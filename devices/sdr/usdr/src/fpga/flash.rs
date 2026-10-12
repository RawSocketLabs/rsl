//! The QSPI flash controller: the gateware core through which the host reads, erases and
//! programs the serial NOR flash the FPGA boots from.
//!
//! The controller has a command/status register, an address/data register and a 128-byte
//! transfer buffer mapped into the register space. A command names a JEDEC opcode, how
//! many bytes it moves and whether they go through the data register or the buffer. The
//! host polls the controller until a command has run, and the flash's own status until an
//! erase or program has finished.
//!
//! Source: libusdr's `ipblks/espi_flash.c` (`espi_flash.c`:N below is a line there), which
//! this module follows call for call, including its poll limits and waits. Addresses: the
//! uSDR's `/ll/qspi_flash/base` (`M2PCI_REG_QSPI_FLASH`) in `device/m2_lm6_1/m2_lm6_1.c`,
//! and the buffer base of 512 that libusdr's callers pass (`json_controller/controller_ugen.c`).
//! No gateware documentation is vendored, so the fields below carry only the meaning
//! libusdr's use of them shows.
//!
//! Divergences, each rejected before any bus access where libusdr would go ahead:
//! - Ranges must lie within the 16 MiB a 24-bit address reaches (the core's ID is
//!   `USDR_QSPI_FLASH_24_RW`); libusdr sends any 32-bit address.
//! - An erase must cover whole 4 KiB subsectors. libusdr erases block by block and fails
//!   only at the first misaligned remainder, after erasing what came before.
//! - A program chunk must not cross a 256-byte flash page: NOR flash wraps within the page,
//!   which libusdr would let corrupt the page's start.
//! - Skipping the header of an image of at most 256 bytes leaves nothing to write. libusdr
//!   also refuses exactly 256, but below that, when not also erasing, it underflows the
//!   length.
//! - A transfer buffer that reads back differently fails with
//!   [`FlashError::StagingMismatch`] rather than libusdr's `assert` (or, without asserts,
//!   programming it anyway).
//! - A partial last word is staged padded with `0xff`. The padding only reaches the
//!   transfer buffer (the program length excludes it), so it just makes the trace
//!   deterministic; libusdr reads up to three bytes past the caller's buffer there, and
//!   writes up to three past it when reading.
//! - [`FlashId`] reports capacities whole; libusdr's 32-bit arithmetic wraps from 512 MiB.

use std::fmt;
use std::time::Duration;

use bnb::{BitEnum, bitfield, u2};

use crate::error::Access;
use crate::lowlevel::{Bus, BusError};

/// The flash addresses the controller reaches: 24 bits, 16 MiB.
pub const ADDRESS_SPACE: u32 = 1 << 24;
/// The smallest erase, in bytes: a 4 KiB subsector (`ESPI_CMD_SSE`, `espi_flash.c`:242).
pub const SUBSECTOR_BYTES: u32 = 0x1000;
/// The erase used where alignment allows, in bytes: a 64 KiB sector (`espi_flash.c`:243).
pub const SECTOR_BYTES: u32 = 0x1_0000;
/// The flash's program page, in bytes: no program chunk may cross one. JEDEC serial-NOR
/// convention (libusdr does not state it).
pub const PAGE_BYTES: u32 = 256;
/// The image header [`WriteOptions::skip_header`] leaves unwritten, in bytes
/// (`espi_flash.c`:340-344).
pub const HEADER_BYTES: usize = 256;

/// Bytes moved per command: the transfer buffer's size (`LOCAL_BLK_LEN`, `espi_flash.c`:39).
const CHUNK_BYTES: usize = 128;
/// Transfer buffer words.
const CHUNK_WORDS: usize = CHUNK_BYTES / 4;
/// Controller status polls after the first before giving up (`espi_flash.c`:54).
const CONTROLLER_POLL_LIMIT: u32 = 1_000_000;
/// Flash status polls before giving up (`WATCHDOG_LIMIT`, `espi_flash.c`:41).
const FLASH_POLL_LIMIT: u32 = 300_000;
/// The wait after every status poll (`espi_flash.c`:57, 230).
const POLL_INTERVAL: Duration = Duration::from_micros(1);
/// The wait after starting a program (`espi_flash.c`:315).
const PROGRAM_WAIT: Duration = Duration::from_millis(1);
/// The wait after starting a subsector erase (`espi_flash.c`:290).
const SUBSECTOR_ERASE_WAIT: Duration = Duration::from_millis(1);
/// The wait after starting a sector erase (`espi_flash.c`:275).
const SECTOR_ERASE_WAIT: Duration = Duration::from_millis(10);

/// The controller's registers on the uSDR.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reg {
    /// Write: starts a [`Command`]. `ESPI_CMD`, base + 0.
    Command,

    /// Read: [`ControllerStatus`]. `ESPI_STAT`, base + 0.
    Status,

    /// Write: the flash address the next addressed command uses. `ESPI_FADDR`, base + 1.
    Address,

    /// Read: the four bytes the last register command received, first byte in bits 7:0
    /// (`espi_flash.c`:84, 100-102). `ESPI_DATA`, base + 1.
    Data,

    /// The transfer buffer's first word; word `i` holds bytes `4i..4i + 4`, low byte
    /// first, as libusdr's byte copies through the register transport see them
    /// (`espi_flash.c`:170, 351). `cfg_mmap_base`.
    Buffer,
}

impl Reg {
    /// The controller's base on the uSDR: `/ll/qspi_flash/base`, `M2PCI_REG_QSPI_FLASH`.
    const BASE: u32 = 10;

    /// The transfer buffer's base on the uSDR: the `cfg_mmap_base` of 512 that every
    /// libusdr caller passes (`controller_ugen.c`:42, 99).
    const BUFFER: u32 = 512;

    /// The register's address.
    const fn addr(self) -> u32 {
        match self {
            Self::Command | Self::Status => Self::BASE,
            Self::Address | Self::Data => Self::BASE + 1,
            Self::Buffer => Self::BUFFER,
        }
    }
}

/// The JEDEC opcodes libusdr sends (`enum jedec_cmds`, `espi_flash.c`:13-25). It also names
/// read ID 0x9e (`ESPI_CMD_RDID_0`) but never sends it.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
enum Opcode {
    /// Reads from the address into the buffer. `ESPI_CMD_QCFR_0`.
    FastRead = 0x0b,

    /// Sets the write enable latch an erase or program needs. `ESPI_CMD_WREN`.
    WriteEnable = 0x06,

    /// Programs the buffer at the address. `ESPI_CMD_QCPP_0`.
    PageProgram = 0x02,

    /// Erases the 4 KiB subsector at the address. `ESPI_CMD_SSE`.
    SubsectorErase = 0x20,

    /// Erases the 64 KiB sector at the address. `ESPI_CMD_SE`.
    SectorErase = 0xd8,

    /// Erases the whole flash. `ESPI_CMD_BE`.
    ChipErase = 0xc7,

    /// Reads the flash status; see [`FlashStatus`]. `ESPI_CMD_RDSR`.
    ReadStatus = 0x05,

    /// Reads the JEDEC ID; see [`FlashId`]. `ESPI_CMD_RDID_1`.
    ReadId = 0x9f,
}

/// Whether a command sends the address register to the flash (`ab`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2, closed)]
#[repr(u8)]
enum AddressPhase {
    /// No address.
    None = 0,

    /// The address register. libusdr's only other value; what 1 and 3 select is not
    /// documented.
    Address = 2,
}

/// A command register write (`MAKE_ESPI_CORE_CMD`, `espi_flash.c`:27-29). Bits 15:4 (`la16`)
/// are the buffer offset in 16-byte units, which libusdr always leaves 0
/// (`LOCAL_MEM_TOP`).
#[bitfield(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Command {
    /// The opcode sent to the flash. Bits 31:24.
    #[bits(24..=31)]
    opcode: Opcode,

    /// Bytes to move. `sz`, bits 23:16.
    #[bits(16..=23)]
    len: u8,

    /// The bytes move through the transfer buffer rather than the data register. `lbe`,
    /// bit 3.
    #[bits(3..=3)]
    via_buffer: bool,

    /// Whether the address register is sent. `ab`, bits 2:1.
    #[bits(1..=2)]
    address: AddressPhase,

    /// The bytes move from the buffer to the flash (a program) rather than back. `wb`,
    /// bit 0.
    #[bits(0..=0)]
    to_flash: bool,
}

/// The controller status register.
#[bitfield(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ControllerStatus {
    /// The controller is still running a command. Bit 0 (`espi_flash.c`:59).
    #[bits(0..=0)]
    busy: bool,
}

/// The flash's status byte, from [`Opcode::ReadStatus`].
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FlashStatus {
    /// An erase or program is running. `RDSR_BUSY_BIT`, bit 0 (`espi_flash.c`:42).
    #[bits(0..=0)]
    write_in_progress: bool,
}

/// The flash's JEDEC ID, as the controller reads it (`espi_flash_get_id`).
///
/// [`Display`](fmt::Display) gives libusdr's description (`_espi_flash_id_to_str`), for
/// example `Micron Serial NOR MT25Q 128 Mb (3.3V)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FlashId(u32);

impl FlashId {
    /// JEDEC manufacturer ID of Micron.
    const MICRON: u8 = 0x20;
    /// JEDEC manufacturer ID of Macronix.
    const MACRONIX: u8 = 0xc2;
    /// JEDEC manufacturer ID of Adesto.
    const ADESTO: u8 = 0x1f;
    /// Micron's memory type for 3.3 V serial NOR (MT25QL).
    const MICRON_NOR_3V3: u8 = 0xba;
    /// Micron's memory type for 1.8 V serial NOR (MT25QU).
    const MICRON_NOR_1V8: u8 = 0xbb;
    /// Adesto's memory type for its SPI/QPI series.
    const ADESTO_SPI_QPI: u8 = 0x42;

    /// The four bytes the controller read, first byte received in bits 7:0.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// The JEDEC manufacturer ID: the first byte.
    #[must_use]
    pub const fn manufacturer(self) -> u8 {
        self.0.to_le_bytes()[0]
    }

    /// The manufacturer's memory type: the second byte.
    #[must_use]
    pub const fn memory_type(self) -> u8 {
        self.0.to_le_bytes()[1]
    }

    /// The capacity code: the third byte, the base-2 logarithm of the size in bytes as
    /// libusdr reads it.
    #[must_use]
    pub const fn capacity_code(self) -> u8 {
        self.0.to_le_bytes()[2]
    }

    /// The size in bytes the capacity code gives, or `None` for a code of 64 or more.
    #[must_use]
    pub const fn capacity_bytes(self) -> Option<u64> {
        1_u64.checked_shl(self.capacity_code() as u32)
    }

    /// Whether libusdr knows the part: a Micron MT25Q, any Macronix, or an Adesto SPI/QPI
    /// flash. For any other, libusdr returns `-ENODATA` alongside the ID.
    #[must_use]
    pub const fn is_recognized(self) -> bool {
        !matches!(self.family(), Family::Unknown)
    }

    /// The part family, as libusdr tells them apart (`espi_flash.c`:105-119).
    const fn family(self) -> Family {
        match (self.manufacturer(), self.memory_type()) {
            (Self::MICRON, Self::MICRON_NOR_3V3) => Family::Micron { volts: "3.3V" },
            (Self::MICRON, Self::MICRON_NOR_1V8) => Family::Micron { volts: "1.8V" },
            (Self::MACRONIX, _) => Family::Macronix,
            (Self::ADESTO, Self::ADESTO_SPI_QPI) => Family::Adesto,
            _ => Family::Unknown,
        }
    }
}

impl fmt::Display for FlashId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let megabits = Megabits(self.capacity_bytes());
        match self.family() {
            Family::Micron { volts } => {
                write!(f, "Micron Serial NOR MT25Q {megabits} ({volts})")
            }
            Family::Macronix => {
                write!(f, "Macronix MX{:02x} series {megabits}", self.memory_type())
            }
            Family::Adesto => write!(f, "Adesto SPI/QPI series {megabits}"),
            Family::Unknown => write!(
                f,
                "Mfg 0x{:02x} Type 0x{:02x} {megabits}",
                self.manufacturer(),
                self.memory_type()
            ),
        }
    }
}

/// A flash part family libusdr names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    /// Micron MT25Q serial NOR, at its supply voltage.
    Micron {
        /// `3.3V` or `1.8V`.
        volts: &'static str,
    },
    /// Any Macronix part.
    Macronix,
    /// Adesto SPI/QPI series.
    Adesto,
    /// Anything else.
    Unknown,
}

/// A size in bytes shown in whole megabits, rounded down, as libusdr prints it.
struct Megabits(Option<u64>);

impl fmt::Display for Megabits {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(bytes) => write!(f, "{} Mb", bytes >> 17),
            None => f.write_str("? Mb"),
        }
    }
}

/// How [`Programmer::write`](crate::flash::Programmer::write) treats the range it writes:
/// libusdr's `espi_write_flags`. The default erases first and writes every byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WriteOptions {
    /// Erase the range first.
    erase: bool,

    /// Leave the first [`HEADER_BYTES`] unwritten.
    skip_header: bool,
}

impl WriteOptions {
    /// Erase the range first, then write every byte (libusdr's flags 0).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            erase: true,
            skip_header: false,
        }
    }

    /// Do not erase first: the range must already be erased, or hold bits that the data
    /// only clears (`ESPI_FLASH_DONT_ERASE`).
    #[must_use]
    pub const fn without_erase(self) -> Self {
        Self {
            erase: false,
            ..self
        }
    }

    /// Leave the first [`HEADER_BYTES`] of the data unwritten, to write them last
    /// (`ESPI_FLASH_DONT_WRITE_HEADER`). An erase still covers them. What the header holds
    /// is not ported.
    #[must_use]
    pub const fn skip_header(self) -> Self {
        Self {
            skip_header: true,
            ..self
        }
    }
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// A failed flash operation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FlashError {
    /// The range runs past the controller's 16 MiB ([`ADDRESS_SPACE`]). Nothing was sent.
    #[error("flash range {offset:#x} + {len:#x} runs past 16 MiB")]
    #[non_exhaustive]
    OutOfRange {
        /// The first byte.
        offset: u32,

        /// The length in bytes.
        len: usize,
    },

    /// An erase that is not whole 4 KiB subsectors ([`SUBSECTOR_BYTES`]): the offset and a
    /// non-zero length must be multiples of 4 KiB. Nothing was sent.
    #[error("flash erase {offset:#x} + {len:#x} is not whole 4 KiB subsectors")]
    #[non_exhaustive]
    EraseGranularity {
        /// The first byte.
        offset: u32,

        /// The length in bytes.
        len: usize,
    },

    /// A write with no bytes to program: empty, or no longer than the skipped header.
    /// Nothing was sent.
    #[error("flash write of {len} bytes has nothing to program")]
    #[non_exhaustive]
    NothingToWrite {
        /// The data's length in bytes.
        len: usize,
    },

    /// A program chunk would cross a 256-byte flash page ([`PAGE_BYTES`]) and wrap within
    /// it. Nothing was sent.
    #[error("flash program of {len} bytes at {addr:#x} crosses a page")]
    #[non_exhaustive]
    PageCrossing {
        /// The chunk's first byte.
        addr: u32,

        /// The chunk's length in bytes.
        len: usize,
    },

    /// The transfer buffer read back differently from what was written to it, so the chunk
    /// for `addr` was not programmed. Earlier chunks were.
    #[error("flash transfer buffer corrupted before programming {addr:#x}")]
    #[non_exhaustive]
    StagingMismatch {
        /// The chunk's first byte.
        addr: u32,
    },

    /// The controller still reported busy after its poll limit (libusdr's `-ETIMEDOUT`).
    #[error("flash controller stayed busy")]
    ControllerTimeout,

    /// The flash still reported an erase or program running after its poll limit
    /// (libusdr's `-ETIMEDOUT`).
    #[error("flash stayed busy")]
    FlashTimeout,

    /// A controller register access failed on the bus.
    #[error("flash controller register {reg:#x} {access} failed")]
    #[non_exhaustive]
    Bus {
        /// Whether it was a read or a write.
        access: Access,

        /// The register address.
        reg: u32,

        /// The bus failure.
        #[source]
        source: BusError,
    },
}

/// Reads the flash's JEDEC ID (`espi_flash_get_id`).
pub(crate) fn read_id(bus: &mut dyn Bus) -> Result<FlashId, FlashError> {
    read_register(bus, Opcode::ReadId).map(FlashId)
}

/// Fills `out` from flash address `offset`, at most a buffer at a time (`espi_flash_read`).
pub(crate) fn read(bus: &mut dyn Bus, offset: u32, out: &mut [u8]) -> Result<(), FlashError> {
    check_range(offset, out.len())?;
    let mut addr = offset;
    for chunk in out.chunks_mut(CHUNK_BYTES) {
        let len = chunk_len(chunk.len());
        write_reg(bus, Reg::Address, addr)?;
        let read = Command::new()
            .with_opcode(Opcode::FastRead)
            .with_len(len)
            .with_via_buffer(true)
            .with_address(AddressPhase::Address);
        write_reg(bus, Reg::Command, read.to_raw())?;
        wait_for_controller(bus)?;
        let mut words = [0; CHUNK_WORDS];
        let words = &mut words[..chunk.len().div_ceil(4)];
        read_buffer(bus, words)?;
        for (bytes, word) in chunk.chunks_mut(4).zip(words.iter()) {
            bytes.copy_from_slice(&word.to_le_bytes()[..bytes.len()]);
        }
        addr += u32::from(len);
    }
    Ok(())
}

/// Erases `len` bytes from `offset`: 64 KiB sectors where aligned, 4 KiB subsectors
/// elsewhere, waiting for each (`espi_flash_erase`).
pub(crate) fn erase(bus: &mut dyn Bus, offset: u32, len: u32) -> Result<(), FlashError> {
    let len = usize::try_from(len).expect("invariant: u32 fits usize");
    check_range(offset, len)?;
    check_erase(offset, len)?;
    erase_blocks(bus, offset, len)
}

/// Erases the whole flash and waits for it (`espi_flash_erase` of all `0xffffffff` bytes
/// from 0).
pub(crate) fn erase_chip(bus: &mut dyn Bus) -> Result<(), FlashError> {
    enable_writes(bus)?;
    command(bus, Opcode::ChipErase)?;
    wait_for_flash(bus)
}

/// Writes `data` from flash address `offset`, erasing first unless `options` say not to,
/// a buffer at a time (`espi_flash_write`).
pub(crate) fn write(
    bus: &mut dyn Bus,
    offset: u32,
    data: &[u8],
    options: WriteOptions,
) -> Result<(), FlashError> {
    check_range(offset, data.len())?;
    if options.erase {
        check_erase(offset, data.len())?;
    }
    let skip = if options.skip_header { HEADER_BYTES } else { 0 };
    let body = data
        .get(skip..)
        .filter(|body| !body.is_empty())
        .ok_or(FlashError::NothingToWrite { len: data.len() })?;
    let start = offset + u32::try_from(skip).expect("invariant: 0 or 256");
    for (addr, chunk) in chunks(start, body) {
        let len = u32::try_from(chunk.len()).expect("invariant: at most a buffer");
        if addr % PAGE_BYTES + len > PAGE_BYTES {
            return Err(FlashError::PageCrossing {
                addr,
                len: chunk.len(),
            });
        }
    }

    if options.erase {
        erase_blocks(bus, offset, data.len())?;
    }
    for (addr, chunk) in chunks(start, body) {
        stage(bus, addr, chunk)?;
        program(bus, addr, chunk_len(chunk.len()))?;
    }
    Ok(())
}

/// `data` in buffer-sized chunks, each with its flash address from `start`.
fn chunks(start: u32, data: &[u8]) -> impl Iterator<Item = (u32, &[u8])> {
    (start..).step_by(CHUNK_BYTES).zip(data.chunks(CHUNK_BYTES))
}

/// A chunk length as the command's length field holds it.
fn chunk_len(len: usize) -> u8 {
    u8::try_from(len).expect("invariant: a chunk is at most 128 bytes")
}

/// Rejects a range past [`ADDRESS_SPACE`].
fn check_range(offset: u32, len: usize) -> Result<(), FlashError> {
    let end = u64::from(offset) + u64::try_from(len).unwrap_or(u64::MAX);
    if end > u64::from(ADDRESS_SPACE) {
        return Err(FlashError::OutOfRange { offset, len });
    }
    Ok(())
}

/// Rejects an erase that is not whole subsectors.
fn check_erase(offset: u32, len: usize) -> Result<(), FlashError> {
    let subsector = usize::try_from(SUBSECTOR_BYTES).expect("invariant: 4096");
    if offset % SUBSECTOR_BYTES != 0 || len == 0 || len % subsector != 0 {
        return Err(FlashError::EraseGranularity { offset, len });
    }
    Ok(())
}

/// Erases an already checked range block by block, as `_espi_flash_erase` does: a sector
/// where the address is sector-aligned and a whole sector remains, otherwise a subsector;
/// each followed by a fixed wait, then status polls.
fn erase_blocks(bus: &mut dyn Bus, offset: u32, len: usize) -> Result<(), FlashError> {
    let mut addr = offset;
    let mut left = u32::try_from(len).expect("invariant: checked within 16 MiB");
    while left != 0 {
        enable_writes(bus)?;
        let (opcode, block, wait) = if addr % SECTOR_BYTES == 0 && left >= SECTOR_BYTES {
            (Opcode::SectorErase, SECTOR_BYTES, SECTOR_ERASE_WAIT)
        } else {
            (
                Opcode::SubsectorErase,
                SUBSECTOR_BYTES,
                SUBSECTOR_ERASE_WAIT,
            )
        };
        write_reg(bus, Reg::Address, addr)?;
        let erase = Command::new()
            .with_opcode(opcode)
            .with_address(AddressPhase::Address);
        write_reg(bus, Reg::Command, erase.to_raw())?;
        addr += block;
        left -= block;
        bus.sleep(wait);
        wait_for_flash(bus)?;
    }
    Ok(())
}

/// Loads `chunk` into the transfer buffer, padding a partial last word with `0xff`, and
/// reads it back to check it (`espi_flash.c`:351-364).
fn stage(bus: &mut dyn Bus, addr: u32, chunk: &[u8]) -> Result<(), FlashError> {
    let mut words = [0; CHUNK_WORDS];
    let words = &mut words[..chunk.len().div_ceil(4)];
    for (word, bytes) in words.iter_mut().zip(chunk.chunks(4)) {
        let mut padded = [0xff; 4];
        padded[..bytes.len()].copy_from_slice(bytes);
        *word = u32::from_le_bytes(padded);
    }
    let reg = Reg::Buffer.addr();
    bus.write_regs(reg, words)
        .map_err(bus_error(Access::Write, reg))?;
    let mut readback = [0; CHUNK_WORDS];
    let readback = &mut readback[..words.len()];
    read_buffer(bus, readback)?;
    if readback != words {
        return Err(FlashError::StagingMismatch { addr });
    }
    Ok(())
}

/// Programs the buffer's first `len` bytes at `addr` and waits for it (`_espi_flash_write`).
fn program(bus: &mut dyn Bus, addr: u32, len: u8) -> Result<(), FlashError> {
    enable_writes(bus)?;
    write_reg(bus, Reg::Address, addr)?;
    let program = Command::new()
        .with_opcode(Opcode::PageProgram)
        .with_len(len)
        .with_via_buffer(true)
        .with_address(AddressPhase::Address)
        .with_to_flash(true);
    write_reg(bus, Reg::Command, program.to_raw())?;
    bus.sleep(PROGRAM_WAIT);
    wait_for_controller(bus)?;
    wait_for_flash(bus)
}

/// Sets the flash's write enable latch (`_espi_flash_cmd_wren`).
fn enable_writes(bus: &mut dyn Bus) -> Result<(), FlashError> {
    command(bus, Opcode::WriteEnable)
}

/// Sends a command that moves no data and has no address.
fn command(bus: &mut dyn Bus, opcode: Opcode) -> Result<(), FlashError> {
    write_reg(
        bus,
        Reg::Command,
        Command::new().with_opcode(opcode).to_raw(),
    )
}

/// Runs a command that receives four bytes into the data register, and returns them
/// (`_espi_flash_read_reg`).
fn read_register(bus: &mut dyn Bus, opcode: Opcode) -> Result<u32, FlashError> {
    let command = Command::new().with_opcode(opcode).with_len(4);
    write_reg(bus, Reg::Command, command.to_raw())?;
    wait_for_controller(bus)?;
    read_reg(bus, Reg::Data)
}

/// Polls the controller until it is idle (`_espi_flash_wait_done`). As in libusdr, every
/// poll is followed by a wait, the last one included, and a poll past the limit fails even
/// if it reads idle.
fn wait_for_controller(bus: &mut dyn Bus) -> Result<(), FlashError> {
    let mut polls = 0;
    loop {
        let status = ControllerStatus::from_raw(read_reg(bus, Reg::Status)?);
        if polls > CONTROLLER_POLL_LIMIT {
            return Err(FlashError::ControllerTimeout);
        }
        bus.sleep(POLL_INTERVAL);
        polls += 1;
        if !status.busy() {
            return Ok(());
        }
    }
}

/// Polls the flash status until no erase or program runs (`_espi_flash_wait_ready`).
fn wait_for_flash(bus: &mut dyn Bus) -> Result<(), FlashError> {
    for _ in 0..FLASH_POLL_LIMIT {
        // The status byte is the first received; libusdr reads four (`espi_flash.c`:80-84).
        let [status, ..] = read_register(bus, Opcode::ReadStatus)?.to_le_bytes();
        if !FlashStatus::from_raw(status).write_in_progress() {
            return Ok(());
        }
        bus.sleep(POLL_INTERVAL);
    }
    Err(FlashError::FlashTimeout)
}

/// Reads the transfer buffer's first `words.len()` words.
fn read_buffer(bus: &mut dyn Bus, words: &mut [u32]) -> Result<(), FlashError> {
    let reg = Reg::Buffer.addr();
    bus.read_regs(reg, words)
        .map_err(bus_error(Access::Read, reg))
}

/// Reads one controller register.
fn read_reg(bus: &mut dyn Bus, reg: Reg) -> Result<u32, FlashError> {
    let addr = reg.addr();
    let mut word = [0];
    bus.read_regs(addr, &mut word)
        .map_err(bus_error(Access::Read, addr))?;
    Ok(word[0])
}

/// Writes one controller register.
fn write_reg(bus: &mut dyn Bus, reg: Reg, value: u32) -> Result<(), FlashError> {
    let addr = reg.addr();
    bus.write_regs(addr, &[value])
        .map_err(bus_error(Access::Write, addr))
}

/// Attaches the register access to a bus failure.
fn bus_error(access: Access, reg: u32) -> impl FnOnce(BusError) -> FlashError {
    move |source| FlashError::Bus {
        access,
        reg,
        source,
    }
}
