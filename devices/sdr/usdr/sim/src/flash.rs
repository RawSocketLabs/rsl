//! The FPGA's QSPI flash controller and the serial NOR flash behind it.
//!
//! No gateware source or controller documentation is vendored, so the controller is
//! modelled from how libusdr's driver uses it (`ipblks/espi_flash.c` at the pinned commit;
//! `espi_flash.c:N` below is a line there). Each behaviour is an assumption, stated where it
//! is encoded. The flash follows common JEDEC serial-NOR conventions; the uSDR's flash part
//! is not named in the vendored source, so they are assumptions too. The sim models only
//! the command shapes libusdr issues and panics on any other, so a driver that strays from
//! them fails its test instead of meeting invented behaviour.

/// The controller's command (write) and status (read) register: the uSDR's
/// `/ll/qspi_flash/base`, `M2PCI_REG_QSPI_FLASH` (`device/m2_lm6_1/m2_lm6_1.c`).
const REG_CONTROL: u32 = 10;
/// The flash address (write) and data (read) register: base + 1 (`ESPI_FADDR`, `ESPI_DATA`,
/// `espi_flash.c`:31-37).
const REG_ADDRESS: u32 = 11;
/// First register of the memory-mapped transfer buffer: the `cfg_mmap_base` of 512 that
/// libusdr's callers pass (`json_controller/controller_ugen.c`:42).
const REG_BUFFER: u32 = 512;
/// Buffer size in bytes: `LOCAL_BLK_LEN` (`espi_flash.c`:39), the most libusdr moves at once.
/// Assumption: the gateware's buffer is no larger than libusdr uses.
const BUFFER_BYTES: usize = 128;
/// Registers the buffer spans, one 32-bit word each.
const BUFFER_WORDS: u32 = 32;

/// Status register bit 0: the controller is still running a command (`espi_flash.c`:59).
const CONTROLLER_BUSY: u32 = 1;

/// Flash status bit 0: write in progress (`RDSR_BUSY_BIT`, `espi_flash.c`:42).
const STATUS_BUSY: u8 = 1 << 0;
/// Flash status bit 1: write enable latch. JEDEC convention; libusdr never reads it.
const STATUS_WRITE_ENABLED: u8 = 1 << 1;

/// Program page size in bytes. JEDEC serial-NOR convention: a program that runs past the
/// end of its page wraps to the page's start.
const PAGE_BYTES: usize = 256;
/// Subsector (4 KiB) and sector (64 KiB) erase sizes (`espi_flash.c`:242-243).
const SUBSECTOR_BYTES: usize = 0x1000;
/// See [`SUBSECTOR_BYTES`].
const SECTOR_BYTES: usize = 0x1_0000;

/// How long each operation keeps the flash busy, in virtual microseconds. Sim choices, far
/// below real parts' times, so that drivers poll a few times after their fixed waits
/// (1 ms after a program or subsector erase, 10 ms after a sector erase, none after a chip
/// erase: `espi_flash.c`:275, 290, 315).
const PROGRAM_US: u64 = 1_005;
/// See [`PROGRAM_US`].
const SUBSECTOR_ERASE_US: u64 = 1_010;
/// See [`PROGRAM_US`].
const SECTOR_ERASE_US: u64 = 10_010;
/// See [`PROGRAM_US`].
const CHIP_ERASE_US: u64 = 100;

/// JEDEC opcodes libusdr sends (`enum jedec_cmds`, `espi_flash.c`:13-25).
mod opcode {
    /// Fast read (`ESPI_CMD_QCFR_0`).
    pub(super) const FAST_READ: u8 = 0x0b;
    /// Write enable (`ESPI_CMD_WREN`).
    pub(super) const WRITE_ENABLE: u8 = 0x06;
    /// Page program (`ESPI_CMD_QCPP_0`).
    pub(super) const PAGE_PROGRAM: u8 = 0x02;
    /// 4 KiB subsector erase (`ESPI_CMD_SSE`).
    pub(super) const SUBSECTOR_ERASE: u8 = 0x20;
    /// 64 KiB sector erase (`ESPI_CMD_SE`).
    pub(super) const SECTOR_ERASE: u8 = 0xd8;
    /// Whole-chip erase (`ESPI_CMD_BE`).
    pub(super) const CHIP_ERASE: u8 = 0xc7;
    /// Read status (`ESPI_CMD_RDSR`).
    pub(super) const READ_STATUS: u8 = 0x05;
    /// Read ID (`ESPI_CMD_RDID_1`).
    pub(super) const READ_ID: u8 = 0x9f;
}

/// A command word as `MAKE_ESPI_CORE_CMD` packs it (`espi_flash.c`:27-29).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Command {
    /// Bits 31:24, the JEDEC opcode.
    opcode: u8,
    /// Bits 23:16, bytes moved.
    len: u8,
    /// Bits 15:4, the buffer offset in 16-byte units. libusdr always sends 0.
    buffer_offset: u16,
    /// Bit 3: data moves through the buffer rather than the data register. Assumption, from
    /// libusdr setting it exactly for buffer transfers (`espi_flash.c`:144, 312).
    via_buffer: bool,
    /// Bits 2:1. Assumption: 2 sends the address register as the address phase; 0 sends
    /// none. libusdr uses only these (`espi_flash.c`:144, 202, 211, 312).
    address_phase: u8,
    /// Bit 0: data moves from the buffer to the flash. Assumption, from libusdr setting it
    /// only for page programs (`espi_flash.c`:312).
    to_flash: bool,
}

impl Command {
    /// Unpacks a command register write.
    fn decode(word: u32) -> Self {
        let [opcode, len, ..] = word.to_be_bytes();
        Self {
            opcode,
            len,
            buffer_offset: u16::try_from(word >> 4 & 0xfff).expect("invariant: 12 bits"),
            via_buffer: word & 1 << 3 != 0,
            address_phase: u8::try_from(word >> 1 & 3).expect("invariant: 2 bits"),
            to_flash: word & 1 != 0,
        }
    }

    /// The shape libusdr gives `opcode` (`len` aside, where it varies), or `None` for an
    /// opcode it never sends.
    fn expected(opcode: u8, len: u8) -> Option<Self> {
        let plain = |len| Self {
            opcode,
            len,
            buffer_offset: 0,
            via_buffer: false,
            address_phase: 0,
            to_flash: false,
        };
        let addressed = Self {
            address_phase: 2,
            ..plain(0)
        };
        let buffered = Self {
            len,
            via_buffer: true,
            ..addressed
        };
        Some(match opcode {
            // `espi_flash.c`:80, 128: four bytes into the data register.
            opcode::READ_STATUS | opcode::READ_ID => plain(4),
            // `espi_flash.c`:187, 194.
            opcode::WRITE_ENABLE | opcode::CHIP_ERASE => plain(0),
            // `espi_flash.c`:202, 211.
            opcode::SUBSECTOR_ERASE | opcode::SECTOR_ERASE => addressed,
            // `espi_flash.c`:144: up to `LOCAL_BLK_LEN` bytes into the buffer.
            opcode::FAST_READ => buffered,
            // `espi_flash.c`:312.
            opcode::PAGE_PROGRAM => Self {
                to_flash: true,
                ..buffered
            },
            _ => return None,
        })
    }
}

/// A serial NOR flash chip.
#[derive(Clone, Debug)]
pub struct SimFlash {
    /// The word read ID returns, first byte received in bits 7:0: libusdr takes the
    /// manufacturer from bits 7:0, the memory type from 15:8 and the capacity code from
    /// 23:16 (`espi_flash.c`:100-102).
    id: u32,
    /// The memory array; addresses wrap at its size.
    memory: Vec<u8>,
    /// The write enable latch.
    write_enabled: bool,
    /// Virtual time at which the running erase or program finishes.
    busy_until_us: u64,
    /// Erases and programs never finish.
    stuck: bool,
}

impl SimFlash {
    /// An erased (all `0xff`) flash of `capacity` bytes answering read ID with `id`.
    ///
    /// # Panics
    ///
    /// If `capacity` is not a power of two of at least 64 KiB.
    #[must_use]
    pub fn new(id: u32, capacity: usize) -> Self {
        assert!(
            capacity.is_power_of_two() && capacity >= SECTOR_BYTES,
            "flash capacity must be a power of two of at least one sector"
        );
        Self {
            id,
            memory: vec![0xff; capacity],
            write_enabled: false,
            busy_until_us: 0,
            stuck: false,
        }
    }

    /// The flash with `bytes` stored from `offset`, as if programmed before the test.
    ///
    /// # Panics
    ///
    /// If the bytes run past the end of the flash.
    #[must_use]
    pub fn with_contents(mut self, offset: usize, bytes: &[u8]) -> Self {
        self.memory[offset..offset + bytes.len()].copy_from_slice(bytes);
        self
    }

    /// The flash with every erase and program running forever, so status polls time out.
    #[must_use]
    pub fn stuck_busy(mut self) -> Self {
        self.stuck = true;
        self
    }

    /// The memory array.
    #[must_use]
    pub fn contents(&self) -> &[u8] {
        &self.memory
    }

    /// Whether an erase or program is running at `now_us`.
    fn busy(&self, now_us: u64) -> bool {
        now_us < self.busy_until_us
    }

    /// The status byte: write in progress and write enable latch.
    fn status(&self, now_us: u64) -> u8 {
        let busy = if self.busy(now_us) { STATUS_BUSY } else { 0 };
        let enabled = if self.write_enabled {
            STATUS_WRITE_ENABLED
        } else {
            0
        };
        busy | enabled
    }

    /// Starts an erase or program, if the write enable latch allows: JEDEC convention is
    /// that both need it and clear it.
    fn start(&mut self, now_us: u64, duration_us: u64) -> bool {
        if !self.write_enabled {
            return false;
        }
        self.write_enabled = false;
        self.busy_until_us = if self.stuck {
            u64::MAX
        } else {
            now_us + duration_us
        };
        true
    }

    /// The array index of flash address `addr`: addresses wrap at the array size.
    fn index(&self, addr: u32) -> usize {
        usize::try_from(addr).expect("invariant: u32 fits usize") & (self.memory.len() - 1)
    }

    /// Erases the `block`-byte block holding `addr` to `0xff`.
    fn erase(&mut self, addr: u32, block: usize) {
        let start = self.index(addr) & !(block - 1);
        self.memory[start..start + block].fill(0xff);
    }

    /// Programs `bytes` from `addr`: each bit can only go from 1 to 0, and the bytes wrap
    /// within `addr`'s page.
    fn program(&mut self, addr: u32, bytes: &[u8]) {
        let page = self.index(addr) & !(PAGE_BYTES - 1);
        let mut column = self.index(addr) - page;
        for &byte in bytes {
            self.memory[page + column] &= byte;
            column = (column + 1) % PAGE_BYTES;
        }
    }
}

/// The FPGA's flash controller: three registers, a transfer buffer, and the flash.
#[derive(Debug)]
pub(crate) struct FlashController {
    /// The flash chip.
    pub(crate) chip: SimFlash,
    /// The last flash address written.
    address: u32,
    /// The bytes the last data-register command received.
    data: u32,
    /// The transfer buffer.
    buffer: [u8; BUFFER_BYTES],
    /// The status register reports busy forever.
    pub(crate) stuck: bool,
}

impl FlashController {
    /// A controller in front of `chip`.
    pub(crate) fn new(chip: SimFlash) -> Self {
        Self {
            chip,
            address: 0,
            data: 0,
            buffer: [0; BUFFER_BYTES],
            stuck: false,
        }
    }

    /// Whether register `addr` belongs to the controller.
    pub(crate) fn owns(addr: u32) -> bool {
        matches!(addr, REG_CONTROL | REG_ADDRESS)
            || (REG_BUFFER..REG_BUFFER + BUFFER_WORDS).contains(&addr)
    }

    /// Reads a controller register.
    pub(crate) fn read(&self, addr: u32) -> u32 {
        match addr {
            // Assumption: commands finish before the next register access, so the status
            // reads idle unless the controller is stuck.
            REG_CONTROL => {
                if self.stuck {
                    CONTROLLER_BUSY
                } else {
                    0
                }
            }
            REG_ADDRESS => self.data,
            // Assumption: buffer word i holds bytes 4i..4i+4, little-endian, as libusdr's
            // byte copies through the register transport see them (`espi_flash.c`:170, 351).
            _ => u32::from_le_bytes(self.buffer_word(addr).try_into().expect("4 bytes")),
        }
    }

    /// Writes a controller register at virtual time `now_us`.
    pub(crate) fn write(&mut self, addr: u32, value: u32, now_us: u64) {
        match addr {
            REG_CONTROL => self.run(Command::decode(value), now_us),
            REG_ADDRESS => self.address = value,
            _ => self
                .buffer_word_mut(addr)
                .copy_from_slice(&value.to_le_bytes()),
        }
    }

    /// The buffer bytes behind buffer register `addr`.
    fn buffer_word(&self, addr: u32) -> &[u8] {
        let start = usize::try_from(addr - REG_BUFFER).expect("invariant: small") * 4;
        &self.buffer[start..start + 4]
    }

    /// See [`Self::buffer_word`].
    fn buffer_word_mut(&mut self, addr: u32) -> &mut [u8] {
        let start = usize::try_from(addr - REG_BUFFER).expect("invariant: small") * 4;
        &mut self.buffer[start..start + 4]
    }

    /// Runs one command. JEDEC convention: while an erase or program runs, the flash
    /// answers only read status and ignores everything else; an ignored read sees the
    /// idle-high bus (`0xff`).
    fn run(&mut self, command: Command, now_us: u64) {
        assert_eq!(
            Some(command),
            Command::expected(command.opcode, command.len),
            "the sim models only the command shapes libusdr's espi_flash.c sends"
        );
        let len = usize::from(command.len);
        let busy = self.chip.busy(now_us);
        match command.opcode {
            // Assumption: a NOR flash repeats its status byte for as long as it is clocked.
            opcode::READ_STATUS => {
                self.data = u32::from_le_bytes([self.chip.status(now_us); 4]);
            }
            _ if busy => {
                if command.opcode == opcode::READ_ID {
                    self.data = u32::MAX;
                } else if command.opcode == opcode::FAST_READ {
                    self.buffer_span(len).fill(0xff);
                }
            }
            opcode::READ_ID => self.data = self.chip.id,
            opcode::WRITE_ENABLE => self.chip.write_enabled = true,
            opcode::CHIP_ERASE => {
                if self.chip.start(now_us, CHIP_ERASE_US) {
                    self.chip.memory.fill(0xff);
                }
            }
            opcode::SUBSECTOR_ERASE => {
                if self.chip.start(now_us, SUBSECTOR_ERASE_US) {
                    self.chip.erase(self.address, SUBSECTOR_BYTES);
                }
            }
            opcode::SECTOR_ERASE => {
                if self.chip.start(now_us, SECTOR_ERASE_US) {
                    self.chip.erase(self.address, SECTOR_BYTES);
                }
            }
            // JEDEC convention: a read runs on past the end of the array from address 0.
            opcode::FAST_READ => {
                let bytes: Vec<u8> = (self.address..)
                    .take(len)
                    .map(|addr| self.chip.memory[self.chip.index(addr)])
                    .collect();
                self.buffer_span(len).copy_from_slice(&bytes);
            }
            opcode::PAGE_PROGRAM => {
                if self.chip.start(now_us, PROGRAM_US) {
                    let bytes = self.buffer_span(len).to_vec();
                    self.chip.program(self.address, &bytes);
                }
            }
            _ => unreachable!("checked against the expected shapes above"),
        }
    }

    /// The first `len` buffer bytes, the span a buffered command moves.
    ///
    /// # Panics
    ///
    /// If `len` exceeds the buffer: libusdr never moves more than `LOCAL_BLK_LEN`.
    fn buffer_span(&mut self, len: usize) -> &mut [u8] {
        assert!(
            len <= BUFFER_BYTES,
            "a {len}-byte transfer overflows the {BUFFER_BYTES}-byte buffer"
        );
        &mut self.buffer[..len]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Micron-style ID word: manufacturer 0x20, type 0xba, 2 MiB.
    const ID: u32 = 0x0015_ba20;
    /// The sim flash size the tests use.
    const SIZE: usize = 2 << 20;

    /// `MAKE_ESPI_CORE_CMD` (`espi_flash.c`:27-29), written out independently.
    fn command(opcode: u8, len: u8, via_buffer: bool, address_phase: u32, to_flash: bool) -> u32 {
        u32::from(opcode) << 24
            | u32::from(len) << 16
            | u32::from(via_buffer) << 3
            | address_phase << 1
            | u32::from(to_flash)
    }

    fn controller() -> FlashController {
        FlashController::new(SimFlash::new(ID, SIZE))
    }

    #[test]
    fn program_only_clears_bits_and_wraps_within_its_page() {
        let mut flash = SimFlash::new(ID, SIZE).with_contents(0x1fe, &[0xf0, 0x0f, 0xff]);
        flash.program(0x1fe, &[0x3c, 0x3c, 0x3c, 0x3c]);
        assert_eq!(&flash.contents()[0x1fe..0x200], &[0x30, 0x0c]);
        assert_eq!(
            &flash.contents()[0x100..0x102],
            &[0x3c, 0x3c],
            "bytes past the page end wrap to its start"
        );
        assert_eq!(flash.contents()[0x200], 0xff, "the next page is untouched");
    }

    #[test]
    fn erase_and_program_need_write_enable_and_report_busy_until_done() {
        let mut ctrl = controller();
        ctrl.write(REG_ADDRESS, 0x1000, 0);
        ctrl.chip.memory[0x1000] = 0;
        ctrl.write(
            REG_CONTROL,
            command(opcode::SUBSECTOR_ERASE, 0, false, 2, false),
            0,
        );
        assert_eq!(
            ctrl.chip.contents()[0x1000],
            0,
            "ignored without write enable"
        );

        ctrl.write(
            REG_CONTROL,
            command(opcode::WRITE_ENABLE, 0, false, 0, false),
            0,
        );
        ctrl.write(
            REG_CONTROL,
            command(opcode::SUBSECTOR_ERASE, 0, false, 2, false),
            0,
        );
        assert_eq!(ctrl.chip.contents()[0x1000], 0xff);
        ctrl.write(
            REG_CONTROL,
            command(opcode::READ_STATUS, 4, false, 0, false),
            1,
        );
        assert_eq!(ctrl.read(REG_ADDRESS) & 0xff, u32::from(STATUS_BUSY));
        ctrl.write(
            REG_CONTROL,
            command(opcode::READ_STATUS, 4, false, 0, false),
            SUBSECTOR_ERASE_US,
        );
        assert_eq!(ctrl.read(REG_ADDRESS), 0, "idle with the latch cleared");
    }

    #[test]
    fn buffered_reads_land_in_the_buffer_low_byte_first() {
        let mut ctrl =
            FlashController::new(SimFlash::new(ID, SIZE).with_contents(0x40, &[1, 2, 3, 4, 5]));
        ctrl.write(REG_ADDRESS, 0x40, 0);
        ctrl.write(
            REG_CONTROL,
            command(opcode::FAST_READ, 5, true, 2, false),
            0,
        );
        assert_eq!(ctrl.read(REG_BUFFER), 0x0403_0201);
        assert_eq!(ctrl.read(REG_BUFFER + 1) & 0xff, 5);
    }

    #[test]
    #[should_panic(expected = "command shapes")]
    fn a_command_shape_libusdr_never_sends_is_not_modelled() {
        controller().write(
            REG_CONTROL,
            command(opcode::FAST_READ, 4, false, 2, false),
            0,
        );
    }
}
