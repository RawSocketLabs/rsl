//! Synthetic gateware images for the flash update tests: Xilinx 7-series configuration
//! bitstreams with the preamble, packets and CRC words libusdr's header parser reads
//! (`ipblks/xlnx_bitstream.c`). Shared by `usdr`'s tests and the oracle's parity tests
//! (`#[path]`-included).
//!
//! They are shaped like a Vivado bitstream, but only what libusdr reads matters: the
//! `IDCODE`, `WBSTAR`, `AXSS` and `CMD` writes and the CRC words. The CRC is CRC-32C over
//! each register write's 32 data bits and then its register's low five bits, least
//! significant first (Xilinx UG470, "Bitstream Composition"), leaving out the registers
//! libusdr leaves out. It is scaffolding: what proves these images valid is libusdr's own
//! parser accepting them (`oracle/tests/image_parity.rs`).

/// Padding before the sync word.
pub(crate) const DUMMY: u32 = 0xffff_ffff;
/// The first bus-width detection word.
pub(crate) const BYTE_ORDER: u32 = 0x0000_00bb;
/// The second bus-width detection word.
pub(crate) const BUS_WIDTH: u32 = 0x1122_0044;
/// The sync word.
pub(crate) const SYNC: u32 = 0xaa99_5566;
/// A type 1 packet that does nothing.
pub(crate) const NOOP: u32 = 0x2000_0000;

/// The CRC register.
pub(crate) const CRC: u16 = 0x00;
/// The frame data input register, written by the bulk of an image.
pub(crate) const FDRI: u16 = 0x02;
/// The command register.
pub(crate) const CMD: u16 = 0x04;
/// The ID code register.
pub(crate) const IDCODE: u16 = 0x0c;
/// The user access (build stamp) register.
pub(crate) const AXSS: u16 = 0x0d;
/// The warm-boot start address register.
pub(crate) const WBSTAR: u16 = 0x10;
/// Registers libusdr leaves out of its CRC.
pub(crate) const CRC_SKIPPED: [u16; 5] = [0x16, 0x0f, 0x12, 0x14, 0x15];

/// The command that resets the CRC.
pub(crate) const RCRC: u32 = 0x07;
/// The command that reloads the FPGA from `WBSTAR`.
pub(crate) const IPROG: u32 = 0x0f;

/// The packet operation that writes.
pub(crate) const WRITE: u32 = 2;

/// The XC7A35T's JTAG ID code (libusdr's `XLNX_DEVID_XC7A35T`).
pub(crate) const XC7A35T: u32 = 0x0362_d093;
/// The XC7A50T's (`XLNX_DEVID_XC7A50T`).
pub(crate) const XC7A50T: u32 = 0x0362_c093;

/// A type 1 packet header.
pub(crate) const fn type1(op: u32, register: u16, count: u32) -> u32 {
    1 << 29 | op << 27 | (register as u32) << 13 | count
}

/// A type 2 packet header.
pub(crate) const fn type2(op: u32, count: u32) -> u32 {
    2 << 29 | op << 27 | count
}

/// A bitstream under construction, with the CRC of its register writes so far.
#[derive(Clone, Debug, Default)]
pub(crate) struct Bitstream {
    /// The words so far.
    words: Vec<u32>,
    /// The CRC since the last CRC word or reset.
    crc: u32,
    /// The register a type 2 packet continues.
    last_register: u16,
}

impl Bitstream {
    /// A bitstream starting with padding, bus-width detection and the sync word.
    pub(crate) fn new() -> Self {
        Self::default()
            .raw(&[DUMMY; 8])
            .raw(&[BYTE_ORDER, BUS_WIDTH, DUMMY, DUMMY, SYNC])
    }

    /// Appends `words` as they are, outside the CRC.
    pub(crate) fn raw(mut self, words: &[u32]) -> Self {
        self.words.extend_from_slice(words);
        self
    }

    /// Writes `values` to `register` in a type 1 packet.
    pub(crate) fn write(mut self, register: u16, values: &[u32]) -> Self {
        let count = u32::try_from(values.len()).expect("short packet");
        self.words.push(type1(WRITE, register, count));
        self.last_register = register;
        self.take(values)
    }

    /// Writes `values` to the last register in a type 2 packet, after an empty type 1.
    pub(crate) fn long_write(mut self, register: u16, values: &[u32]) -> Self {
        self = self.write(register, &[]);
        let count = u32::try_from(values.len()).expect("fits 27 bits");
        self.words.push(type2(WRITE, count));
        self.take(values)
    }

    /// Writes the CRC of the writes so far, which resets it.
    pub(crate) fn crc(mut self) -> Self {
        let crc = self.crc;
        self.words.push(type1(WRITE, CRC, 1));
        self.words.push(crc);
        self.crc = 0;
        self
    }

    /// Writes a CRC word that is wrong.
    pub(crate) fn bad_crc(mut self) -> Self {
        self.crc ^= 1;
        self.crc()
    }

    /// The image's bytes: each word big-endian.
    pub(crate) fn bytes(&self) -> Vec<u8> {
        self.words
            .iter()
            .flat_map(|word| word.to_be_bytes())
            .collect()
    }

    /// Folds `values`, written to the packet's register, into the CRC.
    fn take(mut self, values: &[u32]) -> Self {
        for &value in values {
            self.words.push(value);
            self.crc = match self.last_register {
                CMD if value == RCRC => 0,
                register if CRC_SKIPPED.contains(&register) => self.crc,
                register => fold(self.crc, register, value),
            };
        }
        self
    }
}

/// CRC-32C, reflected (polynomial `0x82f63b78`), over a register write's 37 bits.
fn fold(crc: u32, register: u16, value: u32) -> u32 {
    let message = u64::from(value) | u64::from(register & 0x1f) << 32;
    (0..37).fold(crc, |crc, bit| {
        let feedback = (u64::from(crc) ^ message >> bit) & 1;
        crc >> 1 ^ if feedback == 1 { 0x82f6_3b78 } else { 0 }
    })
}

/// What a synthetic image says about itself.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Facts {
    /// Its FPGA's JTAG ID code.
    pub(crate) device_id: u32,
    /// Its warm-boot address: the master offset for a golden image, 0 for a master image.
    pub(crate) warm_boot_start: u32,
    /// Its build stamp.
    pub(crate) build: u32,
    /// Whether it ends with IPROG, as a golden image does.
    pub(crate) reprograms: bool,
    /// Frame data words: the image's bulk.
    pub(crate) frames: u32,
}

impl Facts {
    /// A golden image for the XC7A35T, warm-booting the uSDR's master offset.
    pub(crate) const fn golden(build: u32) -> Self {
        Self {
            device_id: XC7A35T,
            warm_boot_start: 0x1c_0000,
            build,
            reprograms: true,
            frames: 20_000,
        }
    }

    /// A master image for the XC7A35T.
    pub(crate) const fn master(build: u32) -> Self {
        Self {
            device_id: XC7A35T,
            warm_boot_start: 0,
            build,
            reprograms: false,
            frames: 30_000,
        }
    }

    /// The image as a bitstream, every fact libusdr reads within its first 256 bytes but
    /// IPROG, which ends it, and a correct CRC after the frames.
    pub(crate) fn bitstream(self) -> Bitstream {
        let frames: Vec<u32> = (0..self.frames)
            .map(|i| i.wrapping_mul(0x9e37_79b9))
            .collect();
        let bitstream = Bitstream::new()
            .write(CMD, &[RCRC])
            .raw(&[NOOP])
            .write(WBSTAR, &[self.warm_boot_start])
            .write(CMD, &[0])
            .raw(&[NOOP])
            .write(CMD, &[RCRC])
            .raw(&[NOOP, NOOP])
            .write(0x11, &[0])
            .write(0x13, &[0])
            .write(0x09, &[0x0200_3fe5])
            .write(0x0e, &[0])
            .write(IDCODE, &[self.device_id])
            .write(CMD, &[0x09])
            .raw(&[NOOP])
            .write(0x06, &[0x401])
            .write(0x05, &[0x501])
            .write(AXSS, &[self.build])
            .raw(&[NOOP; 4])
            .write(0x01, &[0])
            .write(CMD, &[0x01])
            .raw(&[NOOP])
            .long_write(FDRI, &frames)
            .crc()
            .write(CMD, &[0x0a])
            .raw(&[NOOP]);
        let bitstream = if self.reprograms {
            bitstream.write(CMD, &[IPROG])
        } else {
            bitstream
        };
        bitstream.write(CMD, &[0x0d]).raw(&[NOOP; 4])
    }

    /// The image's bytes.
    pub(crate) fn bytes(self) -> Vec<u8> {
        self.bitstream().bytes()
    }
}
