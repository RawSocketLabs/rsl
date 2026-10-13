//! `PE_SYNC`'s open and close on the simulated board: what the board is left in. The
//! operation-for-operation comparison with libusdr is `oracle/tests/open_parity.rs`.

mod common;

use common::sim_bus::SimBus;
use rsl_pe_sync::{Device, Error};
use rsl_pe_sync_sim::{I2cAddress, SimBoard};

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
