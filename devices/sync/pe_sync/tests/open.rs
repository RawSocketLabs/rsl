//! `PE_SYNC`'s open and close on the simulated board: what the board is left in, and what
//! the open does after a bus error (libusdr's failure rules in `pe_sync_configure`, which
//! the oracle's fresh boards never trigger). The operation-for-operation comparison with
//! libusdr is `oracle/tests/open_parity.rs`.

mod common;

use common::failing_bus::{Access, FailingBus};
use common::sim_bus::SimBus;
use rsl_pe_sync::{Access as Direction, Device, Error};
use rsl_pe_sync_sim::{I2cAddress, Op, SimBoard};

/// The first power-off write: LEDs off (`IGPO_LED_CTRL` = 0).
const LEDS_OFF: Op = Op::RegWrite {
    addr: 0,
    value: 0x0600_0000,
};

/// The SPI words shifted out on `bus`, in order.
fn spi_words(trace: &[Op], bus: u32) -> Vec<u32> {
    trace
        .iter()
        .filter_map(|op| match *op {
            Op::Spi { bus: on, out, .. } if on == bus => Some(out),
            _ => None,
        })
        .collect()
}

/// Opens a fresh board whose bus fails the `skip + 1`th operation `matches` accepts: the
/// outcome and the board's trace.
fn open_failing(matches: fn(Access) -> bool, skip: usize) -> (Result<Device, Error>, Vec<Op>) {
    let sim = SimBus::new(SimBoard::new());
    let opened = Device::open(FailingBus::new(sim.clone(), matches, skip));
    let trace = sim.board().trace().to_vec();
    (opened, trace)
}

#[test]
fn a_failed_supply_write_powers_the_board_off() {
    let (opened, trace) = open_failing(|access| matches!(access, Access::Write(0, 0x0100_0003)), 0);
    assert!(
        matches!(
            opened,
            Err(Error::Bus {
                chip: "FPGA",
                access: Direction::Write,
                reg: 0,
                ..
            })
        ),
        "{opened:?}"
    );
    assert_eq!(
        trace.get(1),
        Some(&LEDS_OFF),
        "the power-off follows the first write"
    );
}

#[test]
fn a_failed_lmx2820_reset_read_still_reads_its_status_then_powers_off() {
    // lmx2820_create fails at its R0 read; pe_sync still calls lmx2820_read_status.
    let (opened, trace) = open_failing(|access| matches!(access, Access::Spi(2, 0x0080_0000)), 0);
    assert!(
        matches!(
            opened,
            Err(Error::Bus {
                chip: "LMX2820",
                access: Direction::Read,
                reg: 0,
                ..
            })
        ),
        "{opened:?}"
    );
    let wait = trace
        .iter()
        .position(|op| *op == Op::Sleep { us: 2_000_000 })
        .expect("the LMK05318 part ran");
    let after: Vec<u32> = spi_words(&trace[wait..], 2);
    assert_eq!(
        after,
        [0x00cc_0000, 0x00ca_0000, 0x00cb_0000],
        "R76, R74, R75 only"
    );
    assert_eq!(trace.get(wait + 4), Some(&LEDS_OFF), "then the power-off");
}

#[test]
fn a_failed_status_read_after_a_tune_is_ignored_as_in_libusdr() {
    // The second R76 read on bus 2 is the post-tune lmx2820_read_status, "just for
    // logging": its failure skips that status's other reads, nothing else.
    let (opened, trace) = open_failing(|access| matches!(access, Access::Spi(2, 0x00cc_0000)), 1);
    opened.expect("libusdr ignores the post-tune status read's result");
    let lock_reads = spi_words(&trace, 2)
        .into_iter()
        .filter(|&word| word == 0x00ca_0000)
        .count();
    assert_eq!(
        lock_reads, 2,
        "create's status and the lock wait; the last status stopped"
    );
}

#[test]
fn a_failed_lmx1204_create_powers_off_without_its_status_read() {
    let (opened, trace) = open_failing(|access| matches!(access, Access::Spi(4, 0x0056_0000)), 0);
    assert!(
        matches!(
            opened,
            Err(Error::Bus {
                chip: "LMX1204",
                ..
            })
        ),
        "{opened:?}"
    );
    assert_eq!(
        spi_words(&trace, 4),
        [0x0017_6040, 0x00a1_0000],
        "only the status check reached the LMX1204"
    );
    let last_lo = trace
        .iter()
        .rposition(|op| matches!(op, Op::Spi { bus: 5, .. }))
        .expect("the LMX1214 was configured");
    assert_eq!(
        trace.get(last_lo + 1),
        Some(&LEDS_OFF),
        "then the power-off"
    );
}

/// The supply banks the open enables (`IGPO_IN_CTRL` to `IGPO_DISTRIB_CTRL`).
const SUPPLY_BANKS: [u8; 5] = [0, 1, 2, 3, 4];
/// The LED bank (`IGPO_LED_CTRL`).
const LED_BANK: u8 = 6;
/// The LMK05318B.
const LMK05318: I2cAddress = I2cAddress {
    core: 0,
    bus: 0,
    addr: 0x64,
};
/// The second LMK1D1208I (`LRF`), the last chip the open configures.
const LMK1D1208I_LRF: I2cAddress = I2cAddress {
    core: 0,
    bus: 1,
    addr: 0x69,
};

/// Whether every supply bank and the LEDs read 0.
fn powered_off(sim: &SimBus) -> bool {
    let board = sim.board();
    SUPPLY_BANKS
        .into_iter()
        .chain([LED_BANK])
        .all(|bank| board.gpo(bank) == Some(0))
}

#[test]
fn an_open_board_is_powered_with_green_leds_and_close_powers_it_off() {
    let sim = SimBus::new(SimBoard::new());
    let device = Device::open(sim.clone()).expect("the board opens");
    {
        let board = sim.board();
        for bank in SUPPLY_BANKS {
            assert!(
                board.gpo(bank).is_some_and(|data| data != 0),
                "supply bank {bank} on"
            );
        }
        assert_eq!(board.gpo(LED_BANK), Some(0x55), "the four green LEDs");
    }
    device.close().expect("the board closes");
    assert!(powered_off(&sim), "close turns every supply and LED off");
}

#[test]
fn a_board_without_its_network_synchronizer_is_refused_and_powered_off() {
    let sim = SimBus::new(SimBoard::new().without(LMK05318));
    let refused = Device::open(sim.clone());
    assert!(
        matches!(
            refused,
            Err(Error::ChipId {
                chip: "LMK05318",
                found: 0xffff_ffff,
                ..
            })
        ),
        "the idle bus is no LMK05318: {refused:?}"
    );
    assert!(powered_off(&sim), "a failed open powers the board off");
}

#[test]
fn a_missing_fan_out_buffer_fails_the_open_after_the_clock_tree_and_powers_off() {
    let sim = SimBus::new(SimBoard::new().without(LMK1D1208I_LRF));
    let refused = Device::open(sim.clone());
    assert!(
        matches!(
            refused,
            Err(Error::ChipId {
                chip: "LMK1D1208I",
                found: 0xff,
                ..
            })
        ),
        "an idle bus reads revision and device 0xf, both wrong: {refused:?}"
    );
    assert!(powered_off(&sim));
}
