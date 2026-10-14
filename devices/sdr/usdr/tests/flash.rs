//! Boot-flash maintenance on the simulated flash controller: what usdr does beyond
//! libusdr's checks. Parity with libusdr is `oracle/tests/flash_parity.rs`.

mod common {
    pub(crate) mod sim_bus;
}

use std::time::Duration;

use common::sim_bus::SimBus;
use usdr::flash::{Flash, FlashError, MASTER_IMAGE_OFFSET, WriteOptions};
use usdr::lowlevel::{Bus, BusError, I2cAddr, SpiAddr};
use usdr_sim::{BoardRevision, SimBoard, SimFlash};

/// A Micron MT25QL128 (128 Mbit, 3.3 V): manufacturer 0x20, type 0xba, capacity 0x18.
const MT25QL128: u32 = 0x0018_ba20;
/// The sim flash's size: 16 MiB, the controller's whole address space.
const SIZE: usize = 16 << 20;

/// A board with an erased MT25QL128.
fn sim() -> SimBus {
    SimBus::new(SimBoard::new(BoardRevision::Rev3).with_flash(SimFlash::new(MT25QL128, SIZE)))
}

#[test]
fn the_id_names_the_part_as_its_datasheet_does() {
    let mut flash = Flash::new(sim());
    let id = flash.id().expect("reads");
    assert_eq!(id.manufacturer(), 0x20);
    assert_eq!(id.capacity_bytes(), Some(16 << 20));
    assert!(id.is_recognized());
    assert_eq!(id.to_string(), "Micron Serial NOR MT25Q 128 Mb (3.3V)");
}

#[test]
fn an_unknown_part_is_described_by_its_raw_bytes() {
    let sim = SimBus::new(
        SimBoard::new(BoardRevision::Rev3).with_flash(SimFlash::new(0x0018_40ef, 1 << 16)),
    );
    let id = Flash::new(sim).id().expect("reads");
    assert!(!id.is_recognized());
    assert_eq!(id.to_string(), "Mfg 0xef Type 0x40 128 Mb");
}

#[test]
fn a_written_image_reads_back_and_nothing_else_changes() {
    let sim = sim();
    let data: Vec<u8> = (0..0x2000_u32).map(|i| (i % 251) as u8).collect();
    let mut flash = Flash::new(sim.clone());
    flash
        .programmer()
        .write(MASTER_IMAGE_OFFSET, &data, WriteOptions::new())
        .expect("writes");
    let mut back = vec![0; data.len()];
    flash.read(MASTER_IMAGE_OFFSET, &mut back).expect("reads");
    assert_eq!(back, data);

    let board = sim.board();
    let contents = board.flash().expect("fitted").contents();
    let master = MASTER_IMAGE_OFFSET as usize;
    assert!(
        contents[..master].iter().all(|&byte| byte == 0xff)
            && contents[master + data.len()..]
                .iter()
                .all(|&byte| byte == 0xff),
        "only the image's range was programmed"
    );
}

#[test]
fn ranges_past_the_controllers_16_mib_are_refused_before_any_bus_access() {
    let sim = sim();
    let mut flash = Flash::new(sim.clone());
    let end = 16 << 20;
    assert!(matches!(
        flash.read(end - 4, &mut [0; 8]),
        Err(FlashError::OutOfRange { .. })
    ));
    assert!(matches!(
        flash.programmer().erase(end, 0x1000),
        Err(FlashError::OutOfRange { .. })
    ));
    assert!(matches!(
        flash.programmer().erase(0, 0xffff_f000),
        Err(FlashError::OutOfRange { .. })
    ));
    assert!(matches!(
        flash
            .programmer()
            .write(end - 0x80, &[0; 0x100], WriteOptions::new().without_erase()),
        Err(FlashError::OutOfRange { .. })
    ));
    assert!(sim.board().trace().is_empty(), "nothing sent");
    flash
        .read(end - 1, &mut [0])
        .expect("the last byte is in range");
}

#[test]
fn skipping_a_header_with_nothing_after_it_is_refused_where_libusdr_underflows() {
    let sim = sim();
    let options = WriteOptions::new().without_erase().skip_header();
    for len in [0, 1, 255, 256] {
        let result = Flash::new(sim.clone())
            .programmer()
            .write(0, &vec![0; len], options);
        assert!(
            matches!(result, Err(FlashError::NothingToWrite { .. })),
            "{len} bytes: {result:?}"
        );
    }
    assert!(sim.board().trace().is_empty(), "nothing sent");
}

/// A bus whose transfer buffer reads back with bit 0 of its first word flipped.
struct CorruptBuffer(SimBus);

impl Bus for CorruptBuffer {
    fn read_regs(&mut self, addr: u32, out: &mut [u32]) -> Result<(), BusError> {
        self.0.read_regs(addr, out)?;
        if addr == 512 {
            out[0] ^= 1;
        }
        Ok(())
    }

    fn write_regs(&mut self, addr: u32, values: &[u32]) -> Result<(), BusError> {
        self.0.write_regs(addr, values)
    }

    fn spi32(&mut self, target: SpiAddr, word: u32) -> Result<u32, BusError> {
        self.0.spi32(target, word)
    }

    fn i2c(&mut self, dev: I2cAddr, write: &[u8], read: &mut [u8]) -> Result<(), BusError> {
        self.0.i2c(dev, write, read)
    }

    fn sleep(&mut self, duration: Duration) {
        self.0.sleep(duration);
    }
}

#[test]
fn a_corrupted_transfer_buffer_is_never_programmed() {
    let sim = sim();
    let result = Flash::new(CorruptBuffer(sim.clone())).programmer().write(
        0x1000,
        &[0; 16],
        WriteOptions::new().without_erase(),
    );
    assert!(
        matches!(
            result,
            Err(FlashError::StagingMismatch { addr: 0x1000, .. })
        ),
        "{result:?}"
    );
    let board = sim.board();
    assert!(
        board.flash().expect("fitted").contents()[0x1000..0x1010]
            .iter()
            .all(|&byte| byte == 0xff),
        "the chunk was not programmed"
    );
}
