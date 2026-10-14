//! usdr's raw diagnostic registers against libusdr's: the ATCRBS core register and
//! `/dm/debug/all`'s TX DMA status words.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use std::ffi::CStr;

use sim_bus::SimBus;
use usdr::{DebugChip, Device, Error};
use usdr_oracle::Oracle;
use usdr_sim::{BoardRevision, Op, SimBoard};

#[test]
fn diagnostic_registers_match_libusdr() {
    let mut oracle = Oracle::open(SimBoard::new(BoardRevision::Rev3)).expect("libusdr opens");
    let start = oracle.trace_len();
    oracle
        .set_atcrbs_register(0x1234_5678)
        .expect("libusdr writes");
    let atcrbs = oracle
        .get_uint(c"/dm/sdr/0/core/atcrbs/reg")
        .expect("libusdr reads");
    let debug = oracle.debug_all().expect("libusdr reads the status");
    let end = oracle.trace_len();
    let reference = oracle.close();

    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let mut device = Device::with_bus(sim.clone()).expect("usdr powers up");
    let actual_start = sim.board().trace().len();
    device
        .set_atcrbs_register(0x1234_5678)
        .expect("usdr writes");
    let value = device.atcrbs_register().expect("usdr reads");
    let status = device.tx_dma_status().expect("usdr reads the status");
    let board = sim.board();

    assert_eq!(u64::from(value), atcrbs, "ATCRBS value");
    let packed = [
        u64::from(status[1]) << 32 | u64::from(status[0]),
        u64::from(status[3]) << 32 | u64::from(status[2]),
    ];
    assert_eq!(packed, debug, "status words, as libusdr packs them");
    let expected: &[Op] = &reference.trace()[start..end];
    let actual: &[Op] = &board.trace()[actual_start..];
    assert_eq!(expected, actual, "operations");
}

/// What libusdr's or usdr's raw chip accesses did and returned.
struct ChipAccesses {
    /// The values read, in order.
    values: Vec<u64>,
    /// The operations.
    ops: Vec<Op>,
}

/// libusdr's raw chip accesses: `(path, packed word)` pairs, each set, then read back.
fn libusdr_chip_accesses(accesses: &[(&CStr, u64)]) -> ChipAccesses {
    let mut oracle = Oracle::open(SimBoard::new(BoardRevision::Rev3)).expect("libusdr opens");
    let start = oracle.trace_len();
    let values = accesses
        .iter()
        .map(|&(path, word)| {
            oracle
                .debug_chip_register(path, word)
                .expect("libusdr accesses the chip")
        })
        .collect();
    let end = oracle.trace_len();
    let board = oracle.close();
    ChipAccesses {
        values,
        ops: board.trace()[start..end].to_vec(),
    }
}

/// usdr's: each `(chip, register, Some(value))` writes and `None` reads, giving the
/// byte read (or the byte written).
fn rsl_chip_accesses(accesses: &[(DebugChip, u8, Option<u8>)]) -> ChipAccesses {
    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let mut device = Device::with_bus(sim.clone()).expect("usdr powers up");
    let start = sim.board().trace().len();
    let values = accesses
        .iter()
        .map(|&(chip, reg, write)| match write {
            Some(value) => {
                device
                    .set_chip_register(chip, reg, value)
                    .expect("usdr writes");
                u64::from(value)
            }
            None => u64::from(device.chip_register(chip, reg).expect("usdr reads")),
        })
        .collect();
    let ops = sim.board().trace()[start..].to_vec();
    ChipAccesses { values, ops }
}

#[test]
fn lms6002d_and_si5332_debug_registers_match_libusdr() {
    const LMS: &CStr = c"/debug/hw/lms6002d/0/reg";
    const SI: &CStr = c"/debug/hw/si5332/0/reg";
    // LMS6002D: the SPI word (bit 15 write, address 14:8, data 7:0). Si5332: bit 23
    // write, address 15:8, data 7:0.
    let reference = libusdr_chip_accesses(&[
        (LMS, 0x0400),
        (LMS, 0x8000 | 0x5a << 8 | 0x3c),
        (LMS, 0x5a00),
        (SI, 0x0700),
        (SI, 0x80_0000 | 0x24 << 8 | 0x81),
        (SI, 0x2400),
    ]);
    let actual = rsl_chip_accesses(&[
        (DebugChip::Lms6002d, 0x04, None),
        (DebugChip::Lms6002d, 0x5a, Some(0x3c)),
        (DebugChip::Lms6002d, 0x5a, None),
        (DebugChip::Si5332, 0x07, None),
        (DebugChip::Si5332, 0x24, Some(0x81)),
        (DebugChip::Si5332, 0x24, None),
    ]);

    assert_eq!(actual.ops, reference.ops, "operations");
    // libusdr's getter after a write returns the SPI reply or 0xff, not the byte written,
    // so only reads are compared.
    for read in [0, 2, 3, 5] {
        assert_eq!(
            actual.values[read], reference.values[read],
            "read {read} returns libusdr's byte"
        );
    }
    assert_eq!(actual.values[2], 0x3c, "the LMS6002D register reads back");
    assert_eq!(actual.values[5], 0x81, "the Si5332 register reads back");
}

#[test]
fn a_tps6381x_debug_write_reaches_the_register_libusdr_misaddresses() {
    const TPS: &CStr = c"/debug/hw/tps6381x/0/reg";
    // Bit 15 is libusdr's write flag, and it stays in the register byte it sends.
    let reference = libusdr_chip_accesses(&[(TPS, 0x0300), (TPS, 0x8000 | 0x05 << 8 | 0x2a)]);
    let actual = rsl_chip_accesses(&[
        (DebugChip::Tps6381x, 0x03, None),
        (DebugChip::Tps6381x, 0x05, Some(0x2a)),
    ]);

    assert_eq!(actual.values[0], reference.values[0], "DEVID");
    assert_eq!(actual.ops[0], reference.ops[0], "the read");
    assert_eq!(actual.ops.len(), 2, "a read, then a write");
    assert_eq!(reference.ops.len(), 2, "a read, then a write");
    let (Op::I2c { write: ours, .. }, Op::I2c { write: theirs, .. }) =
        (&actual.ops[1], &reference.ops[1])
    else {
        panic!("both writes are I2C transfers");
    };
    assert_eq!(theirs, &[0x85, 0x2a], "libusdr writes register 0x85");
    assert_eq!(ours, &[0x05, 0x2a], "usdr writes the register named");
}

#[test]
fn an_lms6002d_address_past_seven_bits_is_refused_before_the_bus() {
    let sim = SimBus::new(SimBoard::new(BoardRevision::Rev3));
    let mut device = Device::with_bus(sim.clone()).expect("usdr powers up");
    let start = sim.board().trace().len();
    assert!(matches!(
        device.chip_register(DebugChip::Lms6002d, 0x80),
        Err(Error::NoSuchRegister { reg: 0x80, .. })
    ));
    assert!(matches!(
        device.set_chip_register(DebugChip::Lms6002d, 0xff, 0),
        Err(Error::NoSuchRegister { reg: 0xff, .. })
    ));
    assert_eq!(sim.board().trace().len(), start, "no bus access");
}
