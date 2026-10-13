//! Gateware images: the header libusdr reads from a Xilinx 7-series configuration
//! bitstream, the checks it makes before flashing one, and the image as it is flashed.
//!
//! Source: libusdr's `ipblks/xlnx_bitstream.c` (`xlnx_bitstream.c`:N below), ported
//! decision for decision, so a header that libusdr accepts or refuses is accepted or refused
//! here with the same fields; and `tools/usdr_flash.c` (`usdr_flash.c`:N) for how an image
//! file is checked and padded. libusdr reads only what the update needs: no other
//! bitstream field is interpreted, and none is invented.
//!
//! A bitstream starts with padding (`0xffffffff`), the bus-width detection pattern
//! (`0x000000bb`, `0x11220044`) and a sync word (`0xaa995566`), then packets of big-endian
//! words. A packet header names an operation, a configuration register and a word count;
//! the image's register writes carry the facts below. libusdr optionally checks the
//! bitstream's CRC words against its own CRC-32C over every register write since the last.

use std::fmt;

use bnb::{bitfield, u2, u3, u11, u14, u27};

use super::flash::{ADDRESS_SPACE, HEADER_BYTES, MASTER_IMAGE_OFFSET, SECTOR_BYTES};

/// Padding before the sync word (`W_DUMMY`, `xlnx_bitstream.c`:11).
const DUMMY: u32 = 0xffff_ffff;
/// The first bus-width detection word (`W_BYTEORD`).
const BYTE_ORDER: u32 = 0x0000_00bb;
/// The second bus-width detection word (`W_BUSWIDTH`).
const BUS_WIDTH: u32 = 0x1122_0044;
/// The sync word: packets follow (`W_SYNCWORD`).
const SYNC: u32 = 0xaa99_5566;

/// The packet operation that writes its words to the register (`xlnx_bitstream.c`:156).
const OP_WRITE: u8 = 2;
/// The command register value that reloads the FPGA from the warm-boot address
/// (`XLNX_CMD_IPROG`).
const CMD_IPROG: u32 = 0x0f;
/// The command register value that resets the CRC (`XLNX_CMD_RCRC`).
const CMD_RESET_CRC: u32 = 0x07;
/// libusdr's CRC polynomial: CRC-32C (Castagnoli), reflected (`CRC32C_REFLECTED_POLY`).
const CRC_POLYNOMIAL: u32 = 0x82f6_3b78;

/// The bytes the golden image's first write covers, the boot sector with the warm-boot
/// trampoline, before the rest of the image (`usdr_flash.c`:292-301).
pub(super) const GOLDEN_BOOT_BYTES: u32 = 4096;

/// The configuration registers libusdr's parser treats specially (`enum`,
/// `xlnx_bitstream.c`:18-51). Any other register's writes only count towards the CRC.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
enum ConfigReg {
    /// A write holds the CRC of every register write since the last reset (`XLNX_REG_CRC`).
    Crc = 0x00,
    /// Commands such as IPROG and RCRC (`XLNX_REG_CMD`).
    Command = 0x04,
    /// The FPGA's JTAG ID code the image is built for (`XLNX_REG_IDCODE`).
    IdCode = 0x0c,
    /// The `USR_ACCESS2` build stamp the gateware reports (`XLNX_REG_AXSS`).
    UserAccess = 0x0d,
    /// Left out of libusdr's CRC (`XLNX_REG_CSOB`).
    Csob = 0x0f,
    /// The warm-boot start address IPROG reloads from (`XLNX_REG_WBSTAR`).
    WarmBootStart = 0x10,
    /// Left out of libusdr's CRC (`XLNX_REG_UNK12`).
    Unknown12 = 0x12,
    /// Left out of libusdr's CRC (`XLNX_REG_UNK14`).
    Unknown14 = 0x14,
    /// Left out of libusdr's CRC (`XLNX_REG_UNK15`).
    Unknown15 = 0x15,
    /// Left out of libusdr's CRC (`XLNX_REG_BOOTSTS`).
    BootStatus = 0x16,
}

impl ConfigReg {
    /// Whether `reg` is this register. libusdr compares the header's whole 14-bit field.
    const fn is(self, reg: u16) -> bool {
        reg == self as u16
    }

    /// Whether libusdr's CRC skips writes to `reg` ("Not sure which ones to skip, but so
    /// far it works well", `xlnx_bitstream.c`:72-80).
    const fn crc_skips(reg: u16) -> bool {
        Self::BootStatus.is(reg)
            || Self::Csob.is(reg)
            || Self::Unknown12.is(reg)
            || Self::Unknown14.is(reg)
            || Self::Unknown15.is(reg)
    }
}

/// A packet header word (`xlnx_bitstream.c`:137-154). Type 1 names its register; type 2
/// continues the last type 1's register with a longer count.
#[bitfield(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Packet {
    /// 1 or 2; libusdr refuses any other. Bits 31:29.
    #[bits(29..=31)]
    kind: u3,

    /// The operation; only writes ([`OP_WRITE`]) carry facts. Bits 28:27.
    #[bits(27..=28)]
    op: u2,

    /// A type 1 packet's register. Bits 26:13.
    #[bits(13..=26)]
    register: u14,

    /// A type 1 packet's word count. Bits 10:0.
    #[bits(0..=10)]
    short_count: u11,

    /// A type 2 packet's word count. Bits 26:0.
    #[bits(0..=26)]
    long_count: u27,
}

/// What libusdr reads from an image's header (`xlnx_image_params_t`): the facts its update
/// checks are made of.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ImageHeader {
    /// The FPGA JTAG ID code the image is built for: the last `IDCODE` register write
    /// (`devid`).
    pub device_id: u32,

    /// Where the FPGA reloads from when the image issues IPROG: the last `WBSTAR` register
    /// write (`wbstar`). A golden image points it at the master image; a master image
    /// writes 0.
    pub warm_boot_start: u32,

    /// The build stamp the gateware reports once loaded (`USR_ACCESS2`): the last `AXSS`
    /// register write (`usr_access2`).
    pub build: u32,

    /// The image issues IPROG, reloading the FPGA from [`warm_boot_start`](Self::warm_boot_start)
    /// (`iprog`).
    pub reprograms: bool,
}

/// How [`ImageHeader::parse`] reads an image: libusdr's `xlnx_btstrm_parse_flags`. The
/// default checks no CRC and refuses a packet that runs past the bytes given.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParseOptions {
    /// Check every CRC word, and require one (`XLNX_BSTRM_PARSE_F_CRC_CHECK`).
    check_crc: bool,

    /// Accept a packet cut off by the end of the bytes (`XLNX_BSTRM_ALLOW_CROP`).
    allow_crop: bool,
}

impl ParseOptions {
    /// No CRC check; a cut-off packet is refused (libusdr's flags 0).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            check_crc: false,
            allow_crop: false,
        }
    }

    /// Check each CRC word against the CRC of the register writes before it, and refuse an
    /// image with none (`XLNX_BSTRM_PARSE_F_CRC_CHECK`).
    #[must_use]
    pub const fn check_crc(self) -> Self {
        Self {
            check_crc: true,
            ..self
        }
    }

    /// Accept a packet whose words run past the end of the bytes, as when reading only an
    /// image's first 256 bytes (`XLNX_BSTRM_ALLOW_CROP`).
    #[must_use]
    pub const fn allow_crop(self) -> Self {
        Self {
            allow_crop: true,
            ..self
        }
    }
}

/// Why libusdr refuses an image header. `index` counts big-endian words from the start.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum HeaderFault {
    /// A word before the sync word that is neither padding nor bus-width detection
    /// (libusdr's `-EINVAL`).
    #[error("word {index} ({word:#010x}) before the sync word is not part of a preamble")]
    #[non_exhaustive]
    UnexpectedWord {
        /// The word's position.
        index: usize,
        /// The word.
        word: u32,
    },

    /// Both bus-width detection words did not come before the sync word, or the end
    /// (`-EINVAL`).
    #[error("no bus-width detection before the sync word")]
    MissingPreamble,

    /// A packet header of neither type 1 nor type 2 (`-EINVAL`).
    #[error("word {index} ({word:#010x}) is not a packet header")]
    #[non_exhaustive]
    UnknownPacket {
        /// The header's position.
        index: usize,
        /// The header.
        word: u32,
    },

    /// A packet's words run past the end of the bytes, without
    /// [`ParseOptions::allow_crop`] (`-EINVAL`).
    #[error("the packet at word {index} runs past the end")]
    #[non_exhaustive]
    Truncated {
        /// The packet header's position.
        index: usize,
    },

    /// A CRC word differs from the CRC of the register writes before it (`-EBADMSG`).
    #[error("CRC word {block} is {stored:#010x}, the writes before it give {computed:#010x}")]
    #[non_exhaustive]
    CrcMismatch {
        /// Which CRC word, from 1.
        block: u32,
        /// The CRC libusdr computes.
        computed: u32,
        /// The CRC the image holds.
        stored: u32,
    },

    /// [`ParseOptions::check_crc`] found no CRC word to check (`-EBADMSG`).
    #[error("the image holds no CRC word")]
    NoCrc,

    /// The image writes no `IDCODE` or no `WBSTAR` (`-ENOENT`).
    #[error("the image writes no {}", missing_fields(*.device_id, *.warm_boot_start))]
    #[non_exhaustive]
    MissingFields {
        /// No `IDCODE` write was found.
        device_id: bool,
        /// No `WBSTAR` write was found.
        warm_boot_start: bool,
    },
}

/// Names the fields [`HeaderFault::MissingFields`] lacks.
const fn missing_fields(device_id: bool, warm_boot_start: bool) -> &'static str {
    match (device_id, warm_boot_start) {
        (true, true) => "IDCODE or WBSTAR",
        (true, false) => "IDCODE",
        _ => "WBSTAR",
    }
}

/// An image header libusdr refuses, with the fields read before it stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{fault}")]
pub struct HeaderError {
    /// Why.
    fault: HeaderFault,
    /// The fields read so far.
    partial: ImageHeader,
}

impl HeaderError {
    /// Why the header was refused.
    #[must_use]
    pub const fn fault(&self) -> HeaderFault {
        self.fault
    }

    /// The fields read before the parser stopped, zero where none was: what libusdr
    /// leaves in its result and, for the master image, still compares
    /// ([`UpdateError::AlreadyFlashed`](super::UpdateError::AlreadyFlashed)).
    #[must_use]
    pub const fn partial(&self) -> ImageHeader {
        self.partial
    }
}

impl ImageHeader {
    /// Reads the header of the image in `bytes`, as libusdr's `xlnx_btstrm_parse_header_ex`
    /// does: whole big-endian words (a partial last word is ignored), the preamble, then
    /// every packet to the end.
    ///
    /// # Errors
    ///
    /// [`HeaderError`] wherever libusdr returns an error; see [`HeaderFault`].
    pub fn parse(bytes: &[u8], options: ParseOptions) -> Result<Self, HeaderError> {
        Parser::new(bytes, options).run()
    }

    /// Whether libusdr would flash this image into `slot` beside `golden`, the golden
    /// image's header on the flash (`xlnx_btstrm_iprgcheck`, `xlnx_bitstream.c`:222-250,
    /// with the uSDR's master offset).
    ///
    /// # Errors
    ///
    /// The first of libusdr's checks that fails, in its order; see [`ImageMismatch`].
    pub const fn check_for(&self, slot: Slot, golden: &Self) -> Result<(), ImageMismatch> {
        let to_golden = matches!(slot, Slot::Golden);
        if golden.device_id != self.device_id {
            return Err(ImageMismatch::DeviceId {
                flash: golden.device_id,
                image: self.device_id,
            });
        }
        if self.warm_boot_start == MASTER_IMAGE_OFFSET && !to_golden {
            return Err(ImageMismatch::GoldenImageForMaster);
        }
        if to_golden && (self.warm_boot_start != MASTER_IMAGE_OFFSET || !self.reprograms) {
            return Err(ImageMismatch::NotAGoldenImage);
        }
        if !to_golden && self.warm_boot_start != 0 {
            return Err(ImageMismatch::NotAMasterImage {
                warm_boot_start: self.warm_boot_start,
            });
        }
        if !to_golden && golden.warm_boot_start != MASTER_IMAGE_OFFSET {
            return Err(ImageMismatch::GoldenNotAligned {
                warm_boot_start: golden.warm_boot_start,
            });
        }
        Ok(())
    }
}

/// Where an image goes on the uSDR's flash.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slot {
    /// The golden image at [`GOLDEN_IMAGE_OFFSET`](super::GOLDEN_IMAGE_OFFSET), which the
    /// FPGA loads at power-up and which warm-boots the master image.
    Golden,
    /// The master image at [`MASTER_IMAGE_OFFSET`].
    Master,
}

impl Slot {
    /// The slot's first byte on the flash.
    #[must_use]
    pub const fn offset(self) -> u32 {
        match self {
            Self::Golden => super::GOLDEN_IMAGE_OFFSET,
            Self::Master => MASTER_IMAGE_OFFSET,
        }
    }
}

impl fmt::Display for Slot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Golden => "golden",
            Self::Master => "master",
        })
    }
}

/// Why libusdr would not flash an image into a slot (`xlnx_btstrm_iprgcheck`'s `-EINVAL`s,
/// in its order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ImageMismatch {
    /// The image is built for another FPGA than the golden image on the flash.
    #[error("the image is for FPGA {image:#010x}, the flash's golden image for {flash:#010x}")]
    #[non_exhaustive]
    DeviceId {
        /// The golden image's [`ImageHeader::device_id`].
        flash: u32,
        /// The image's.
        image: u32,
    },

    /// The image warm-boots the master image, as a golden image does, but the master slot
    /// was asked for.
    #[error("the image is a golden image, but the master slot was asked for")]
    GoldenImageForMaster,

    /// The golden slot was asked for, but the image does not warm-boot the master image
    /// with IPROG.
    #[error("the golden slot was asked for, but the image is not a golden image")]
    NotAGoldenImage,

    /// The master slot was asked for, but the image sets a warm-boot address.
    #[error("the master slot was asked for, but the image warm-boots {warm_boot_start:#x}")]
    #[non_exhaustive]
    NotAMasterImage {
        /// The image's [`ImageHeader::warm_boot_start`].
        warm_boot_start: u32,
    },

    /// The golden image on the flash does not warm-boot the master slot, so a master image
    /// would never load: update the golden image first.
    #[error("the flash's golden image warm-boots {warm_boot_start:#x}, not the master image")]
    #[non_exhaustive]
    GoldenNotAligned {
        /// The golden image's [`ImageHeader::warm_boot_start`].
        warm_boot_start: u32,
    },
}

/// A gateware image file, checked as libusdr's update checks it and padded as it flashes
/// it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    /// The file, padded with `0xff` to whole 64 KiB sectors (`usdr_flash.c`:255-259).
    bytes: Vec<u8>,
    /// Its header.
    header: ImageHeader,
}

/// Why an image file cannot be flashed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ImageError {
    /// The file is larger than the controller's 16 MiB ([`ADDRESS_SPACE`]). libusdr reads
    /// it into a 16 MiB buffer regardless, past its end.
    #[error("a {len}-byte image does not fit the 16 MiB flash")]
    #[non_exhaustive]
    TooLarge {
        /// The file's length in bytes.
        len: usize,
    },

    /// The header is refused (libusdr: "It looks like the file is corrupted").
    #[error("image header refused")]
    Header(#[source] HeaderError),
}

impl Image {
    /// Checks `file` as libusdr's update does by default: the whole file's header parsed with
    /// every CRC word checked (`usdr_flash.c`:234-237).
    ///
    /// # Errors
    ///
    /// [`ImageError::TooLarge`] past 16 MiB; [`ImageError::Header`] where libusdr refuses
    /// the file.
    pub fn new(file: &[u8]) -> Result<Self, ImageError> {
        Self::check(file, |file| {
            ImageHeader::parse(file, ParseOptions::new().check_crc())
        })
    }

    /// Checks `file` as libusdr's update does when told not to check CRCs (`-k`): only its
    /// first 256 bytes, `0xff` past a shorter file, cut-off packets accepted.
    ///
    /// # Errors
    ///
    /// As [`Image::new`].
    pub fn without_crc_check(file: &[u8]) -> Result<Self, ImageError> {
        Self::check(file, |file| {
            let mut head = [0xff; HEADER_BYTES];
            let len = file.len().min(HEADER_BYTES);
            head[..len].copy_from_slice(&file[..len]);
            ImageHeader::parse(&head, ParseOptions::new().allow_crop())
        })
    }

    /// The image's header.
    #[must_use]
    pub const fn header(&self) -> ImageHeader {
        self.header
    }

    /// The bytes an update writes: the file, then `0xff` to the next 64 KiB.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Builds the image from `file` once it fits and `parse` accepts its header.
    fn check(
        file: &[u8],
        parse: impl FnOnce(&[u8]) -> Result<ImageHeader, HeaderError>,
    ) -> Result<Self, ImageError> {
        let space = usize::try_from(ADDRESS_SPACE).expect("invariant: 16 MiB fits usize");
        if file.len() > space {
            return Err(ImageError::TooLarge { len: file.len() });
        }
        let header = parse(file).map_err(ImageError::Header)?;
        let sector = usize::try_from(SECTOR_BYTES).expect("invariant: 64 KiB fits usize");
        let mut bytes = file.to_vec();
        bytes.resize(file.len().next_multiple_of(sector), 0xff);
        Ok(Self { bytes, header })
    }
}

/// `xlnx_btstrm_parse_header_ex`'s state over one image.
struct Parser<'a> {
    /// The image's whole words, big-endian.
    words: &'a [u8],
    /// How to read it.
    options: ParseOptions,
    /// The fields read so far.
    header: ImageHeader,
    /// An `IDCODE` write was seen (`devid_found`).
    has_device_id: bool,
    /// A `WBSTAR` write was seen (`wbstar_found`).
    has_warm_boot_start: bool,
    /// The CRC of the register writes since the last CRC word or reset (`stream_crc`).
    crc: u32,
    /// CRC words checked (`crc_word_cnt`).
    crc_words: u32,
}

impl<'a> Parser<'a> {
    /// A parser over `bytes`' whole words.
    fn new(bytes: &'a [u8], options: ParseOptions) -> Self {
        Self {
            words: &bytes[..bytes.len() / 4 * 4],
            options,
            header: ImageHeader::default(),
            has_device_id: false,
            has_warm_boot_start: false,
            crc: 0,
            crc_words: 0,
        }
    }

    /// The number of whole words.
    const fn len(&self) -> usize {
        self.words.len() / 4
    }

    /// Word `index`, which must exist.
    fn word(&self, index: usize) -> u32 {
        let bytes = self.words[index * 4..index * 4 + 4]
            .try_into()
            .expect("invariant: four bytes");
        u32::from_be_bytes(bytes)
    }

    /// Stops with `fault`, keeping what was read.
    const fn fail(&self, fault: HeaderFault) -> HeaderError {
        HeaderError {
            fault,
            partial: self.header,
        }
    }

    /// Runs the whole parse (`xlnx_bitstream.c`:93-215).
    fn run(mut self) -> Result<ImageHeader, HeaderError> {
        let start = self.preamble()?;
        self.packets(start)?;
        if self.options.check_crc && self.crc_words == 0 {
            return Err(self.fail(HeaderFault::NoCrc));
        }
        if !(self.has_device_id && self.has_warm_boot_start) {
            return Err(self.fail(HeaderFault::MissingFields {
                device_id: !self.has_device_id,
                warm_boot_start: !self.has_warm_boot_start,
            }));
        }
        Ok(self.header)
    }

    /// Reads the words before the first packet and returns where packets start: after the
    /// sync word, or at the end if there is none (`xlnx_bitstream.c`:107-132).
    fn preamble(&self) -> Result<usize, HeaderError> {
        let (mut byte_order, mut bus_width) = (false, false);
        let mut index = 0;
        while index < self.len() {
            let word = self.word(index);
            index += 1;
            match word {
                DUMMY => {}
                BYTE_ORDER => byte_order = true,
                BUS_WIDTH => bus_width = true,
                SYNC => break,
                _ => {
                    return Err(self.fail(HeaderFault::UnexpectedWord {
                        index: index - 1,
                        word,
                    }));
                }
            }
        }
        if !(byte_order && bus_width) {
            return Err(self.fail(HeaderFault::MissingPreamble));
        }
        Ok(index)
    }

    /// Reads packets from word `index` to the end (`xlnx_bitstream.c`:134-205). Only writes
    /// with words are followed; any other header is stepped over alone, as libusdr does.
    fn packets(&mut self, mut index: usize) -> Result<(), HeaderError> {
        let mut last_register = ConfigReg::Crc as u16;
        while index < self.len() {
            let word = self.word(index);
            let packet = Packet::from_raw(word);
            let (register, count) = match packet.kind().value() {
                1 => {
                    last_register = packet.register().value();
                    (last_register, u32::from(packet.short_count().value()))
                }
                2 => (last_register, packet.long_count().value()),
                _ => return Err(self.fail(HeaderFault::UnknownPacket { index, word })),
            };
            let header = index;
            if packet.op().value() == OP_WRITE {
                for _ in 0..count {
                    index += 1;
                    if index >= self.len() {
                        if self.options.allow_crop {
                            break;
                        }
                        return Err(self.fail(HeaderFault::Truncated { index: header }));
                    }
                    self.register_write(register, self.word(index))?;
                }
            }
            index += 1;
        }
        Ok(())
    }

    /// Takes one word written to `register` (`xlnx_bitstream.c`:168-203).
    fn register_write(&mut self, register: u16, value: u32) -> Result<(), HeaderError> {
        if ConfigReg::IdCode.is(register) {
            self.header.device_id = value;
            self.has_device_id = true;
        } else if ConfigReg::WarmBootStart.is(register) {
            self.header.warm_boot_start = value;
            self.has_warm_boot_start = true;
        } else if ConfigReg::Command.is(register) {
            if value == CMD_IPROG {
                self.header.reprograms = true;
            }
        } else if ConfigReg::UserAccess.is(register) {
            self.header.build = value;
        }

        if !self.options.check_crc {
            return Ok(());
        }
        if ConfigReg::Crc.is(register) {
            self.crc_words += 1;
            if value != self.crc {
                return Err(self.fail(HeaderFault::CrcMismatch {
                    block: self.crc_words,
                    computed: self.crc,
                    stored: value,
                }));
            }
            self.crc = 0;
        } else if ConfigReg::Command.is(register) && value == CMD_RESET_CRC {
            self.crc = 0;
        } else {
            self.crc = crc_register_write(self.crc, register, value);
        }
        Ok(())
    }
}

/// Folds one register write into libusdr's CRC: the value's 32 bits, then the register's
/// low five, each least significant first, unless the register is one it skips
/// (`xlnx_btstrm_crc32_regw`, `xlnx_bitstream.c`:59-91).
fn crc_register_write(crc: u32, register: u16, value: u32) -> u32 {
    if ConfigReg::crc_skips(register) {
        return crc;
    }
    let value_bits = (0..32).map(|bit| value >> bit & 1);
    let register_bits = (0..5).map(|bit| u32::from(register >> bit & 1));
    value_bits.chain(register_bits).fold(crc, |crc, bit| {
        let feedback = (crc ^ bit) & 1 != 0;
        let shifted = crc >> 1;
        if feedback {
            shifted ^ CRC_POLYNOMIAL
        } else {
            shifted
        }
    })
}
