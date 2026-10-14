//! The gateware update on the simulated flash: what usdr guarantees beyond libusdr's
//! flash tool, and what the oracle cannot show (a failing bus or flash). Parity with the
//! tool is `oracle/tests/flash_update_parity.rs`; with its header parser,
//! `oracle/tests/image_parity.rs`.

mod common {
    #[allow(
        dead_code,
        reason = "each test crate uses part of the shared image builder"
    )]
    pub(crate) mod bitstream;
    pub(crate) mod sim_bus;
}

use std::time::Duration;

use common::bitstream::{Bitstream, Facts, IDCODE, NOOP, WBSTAR, XC7A35T};
use common::sim_bus::SimBus;
use usdr::flash::{
    ADDRESS_SPACE, Flash, FlashError, Image, ImageError, ImageHeader, MASTER_IMAGE_OFFSET,
    ParseOptions, UpdateError, UpdateOptions,
};
use usdr::lowlevel::{Bus, BusError, I2cAddr, SpiAddr};
use usdr_sim::{BoardRevision, SimBoard, SimFlash};

/// A Micron MT25QL128 (128 Mbit, 3.3 V).
const MT25QL128: u32 = 0x0018_ba20;
/// Its size: the controller's whole 16 MiB.
const SIZE: usize = 16 << 20;
/// The master slot's first byte, as an index.
const MASTER: usize = MASTER_IMAGE_OFFSET as usize;

/// A board running build 2 whose flash holds golden build 1 and master build 2.
fn shipped() -> SimBus {
    let flash = SimFlash::new(MT25QL128, SIZE)
        .with_contents(0, &Facts::golden(1).bytes())
        .with_contents(MASTER, &Facts::master(2).bytes());
    SimBus::new(
        SimBoard::new(BoardRevision::Rev3)
            .with_gateware_build(2)
            .with_flash(flash),
    )
}

/// The board's flash contents.
fn contents(sim: &SimBus) -> Vec<u8> {
    sim.board().flash().expect("fitted").contents().to_vec()
}

/// Whether the board's trace sets the flash's write enable latch, which every erase and
/// program does first.
fn changed_flash(sim: &SimBus) -> bool {
    sim.board()
        .trace()
        .iter()
        .any(|op| matches!(op, usdr_sim::Op::RegWrite { addr: 10, value } if value >> 24 == 0x06))
}

/// A bus that interferes with the `n`th page program command (counting from 0): fails it
/// on the bus, or drops it unsent while reporting success.
struct Faulty {
    /// The board.
    sim: SimBus,
    /// Page program commands sent so far.
    programs: usize,
    /// Which one to interfere with.
    at: usize,
    /// Fail it rather than drop it.
    fail: bool,
}

impl Faulty {
    /// The flash controller's command register.
    const COMMAND: u32 = 10;
    /// The page program opcode, in a command's top byte.
    const PAGE_PROGRAM: u32 = 0x02;
}

impl Bus for Faulty {
    fn read_regs(&mut self, addr: u32, out: &mut [u32]) -> Result<(), BusError> {
        self.sim.read_regs(addr, out)
    }

    fn write_regs(&mut self, addr: u32, values: &[u32]) -> Result<(), BusError> {
        if addr == Self::COMMAND && values[0] >> 24 == Self::PAGE_PROGRAM {
            self.programs += 1;
            if self.programs - 1 == self.at {
                return if self.fail {
                    Err(BusError::Timeout)
                } else {
                    Ok(())
                };
            }
        }
        self.sim.write_regs(addr, values)
    }

    fn spi32(&mut self, target: SpiAddr, word: u32) -> Result<u32, BusError> {
        self.sim.spi32(target, word)
    }

    fn i2c(&mut self, dev: I2cAddr, write: &[u8], read: &mut [u8]) -> Result<(), BusError> {
        self.sim.i2c(dev, write, read)
    }

    fn sleep(&mut self, duration: Duration) {
        self.sim.sleep(duration);
    }
}

#[test]
fn an_interrupted_master_update_leaves_the_golden_image_and_no_master_header() {
    let sim = shipped();
    let before = contents(&sim);
    let image = Image::new(&Facts::master(3).bytes()).expect("valid");
    let faulty = Faulty {
        sim: sim.clone(),
        programs: 0,
        at: 100,
        fail: true,
    };
    let result = Flash::new(faulty)
        .programmer()
        .update_master(&image, UpdateOptions::new());
    assert!(
        matches!(result, Err(UpdateError::Flash(FlashError::Bus { .. }))),
        "the failure is reported: {result:?}"
    );
    let after = contents(&sim);
    assert!(
        after[..MASTER] == before[..MASTER],
        "the golden image is untouched"
    );
    assert!(
        after[MASTER..MASTER + 256].iter().all(|&byte| byte == 0xff),
        "the master slot has no header, so no image the golden image could load"
    );
    let header = ImageHeader::parse(
        &after[MASTER..MASTER + 256],
        ParseOptions::new().allow_crop(),
    );
    assert!(header.is_err(), "the blank master header does not parse");

    // Running the update again recovers it.
    Flash::new(sim.clone())
        .programmer()
        .update_master(&image, UpdateOptions::new())
        .expect("the retry updates");
}

#[test]
fn a_program_that_never_happened_is_reported_where_libusdr_reports_success() {
    let sim = shipped();
    let image = Image::new(&Facts::master(3).bytes()).expect("valid");
    // The body's eleventh chunk: 128 bytes at 256 + 10 * 128 past the slot's start.
    let faulty = Faulty {
        sim: sim.clone(),
        programs: 0,
        at: 10,
        fail: false,
    };
    let result = Flash::new(faulty)
        .programmer()
        .update_master(&image, UpdateOptions::new());
    match result {
        Err(UpdateError::VerifyFailed { blocks, first, .. }) => {
            assert_eq!((blocks, first), (1, MASTER_IMAGE_OFFSET + 0x600));
        }
        other => panic!("the read-back finds the missing chunk: {other:?}"),
    }
}

#[test]
fn an_image_past_the_master_slot_is_refused_before_anything_changes() {
    let sim = shipped();
    let room = usize::try_from(ADDRESS_SPACE).expect("fits") - MASTER;
    // A master image's header, then no-op packets to one byte past the slot.
    let mut file = Bitstream::new()
        .write(IDCODE, &[XC7A35T])
        .write(WBSTAR, &[0])
        .bytes();
    let filler = NOOP.to_be_bytes().into_iter().cycle();
    file.extend(filler.take(room + 1 - file.len()));
    let image = Image::without_crc_check(&file).expect("its header is a master image's");
    let result = Flash::new(sim.clone())
        .programmer()
        .update_master(&image, UpdateOptions::new());
    assert!(
        matches!(
            result,
            Err(UpdateError::Flash(FlashError::OutOfRange { .. }))
        ),
        "{result:?}"
    );
    assert!(!changed_flash(&sim), "nothing was erased or written");

    file.truncate(room);
    let image = Image::without_crc_check(&file).expect("fits");
    assert_eq!(image.as_bytes().len(), room, "a full slot needs no padding");
}

#[test]
fn files_past_16_mib_are_refused() {
    let mut file = Facts::master(3).bytes();
    file.resize(SIZE + 1, 0xff);
    assert!(matches!(
        Image::new(&file),
        Err(ImageError::TooLarge { len, .. }) if len == SIZE + 1
    ));
    assert!(matches!(
        Image::without_crc_check(&file),
        Err(ImageError::TooLarge { .. })
    ));
}

#[test]
fn an_image_is_padded_to_whole_sectors_with_erased_bytes() {
    let file = Facts::master(3).bytes();
    let image = Image::new(&file).expect("valid");
    let padded = image.as_bytes();
    assert_eq!(padded.len(), file.len().next_multiple_of(0x1_0000));
    assert!(padded.starts_with(&file), "the file comes first");
    assert!(padded[file.len()..].iter().all(|&byte| byte == 0xff));
}

#[test]
fn a_master_update_needs_the_golden_image_to_boot_it() {
    // A golden image warm-booting elsewhere would never load the new master image.
    let golden = Facts {
        warm_boot_start: 0x40_0000,
        ..Facts::golden(1)
    };
    let sim = SimBus::new(
        SimBoard::new(BoardRevision::Rev3)
            .with_flash(SimFlash::new(MT25QL128, SIZE).with_contents(0, &golden.bytes())),
    );
    let image = Image::new(&Facts::master(3).bytes()).expect("valid");
    let result = Flash::new(sim.clone())
        .programmer()
        .update_master(&image, UpdateOptions::new());
    assert!(
        matches!(result, Err(UpdateError::Mismatch(_))),
        "{result:?}"
    );
    assert!(!changed_flash(&sim), "nothing was erased or written");
}

/// A seeded xorshift64 generator.
struct Rng(u64);

impl Rng {
    /// The next 64 random bits.
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[test]
fn the_header_parser_never_panics_on_arbitrary_bytes() {
    let mut rng = Rng(0x0dd_b17e_5eed);
    let options = [
        ParseOptions::new(),
        ParseOptions::new().check_crc(),
        ParseOptions::new().allow_crop(),
        ParseOptions::new().check_crc().allow_crop(),
    ];
    for _ in 0..20_000 {
        let len = usize::try_from(rng.next() % 300).expect("small");
        // Words drawn mostly from those the parser acts on, so it gets past the preamble.
        let bytes: Vec<u8> = (0..len.div_ceil(4))
            .flat_map(|_| {
                let word = match rng.next() % 8 {
                    0 => 0xaa99_5566,
                    1 => 0x0000_00bb,
                    2 => 0x1122_0044,
                    3 => NOOP,
                    4 => 0x3000_0000 | u32::try_from(rng.next() & 0x07ff_ffff).expect("27 bits"),
                    5 => 0x5000_0000 | u32::try_from(rng.next() & 0x07ff_ffff).expect("27 bits"),
                    _ => u32::try_from(rng.next() >> 32).expect("32 bits"),
                };
                word.to_be_bytes()
            })
            .take(len)
            .collect();
        for options in options {
            let _ = ImageHeader::parse(&bytes, options);
        }
    }
}
