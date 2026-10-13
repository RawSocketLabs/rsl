//! Sample rate, tuning and stream preparation on the simulated LimeSDR-mini: what libusdr
//! cannot show (its failure paths that divide by zero) and the refusals before any
//! exchange. Operation parity with libusdr itself is `oracle/tests/control_parity.rs`.

mod common;

use common::sim_port::SimPort;
use limesdr::{Device, Error};
use limesdr_sim::{Op, SimBoard};

/// The RF switches, gateware register 0x0017.
const RF_SWITCHES: u16 = 0x0017;

/// An opened LimeSDR-mini on `board`, and the port to inspect it through.
fn lime(board: SimBoard) -> (Device, SimPort) {
    let sim = SimPort::new(board);
    (Device::open(sim.clone()).expect("the board opens"), sim)
}

/// The operations `sim` traced since `from`.
fn since(sim: &SimPort, from: usize) -> Vec<Op> {
    sim.board().trace()[from..].to_vec()
}

#[test]
fn rates_out_of_range_are_refused_before_any_exchange() {
    let (mut device, sim) = lime(SimBoard::new());
    let opened = sim.board().trace().len();
    for (rx, tx) in [(0, 0), (89_999, 0), (0, 122_880_001), (1_000_000, u32::MAX)] {
        assert!(
            matches!(device.set_sample_rate(rx, tx), Err(Error::InvalidRate(_))),
            "RX {rx}, TX {tx}"
        );
    }
    assert!(since(&sim, opened).is_empty(), "nothing exchanged");
}

#[test]
fn streaming_is_not_prepared_when_the_default_rate_does_not_lock() {
    // libusdr ignores the default rate's failure and then divides by zero for the
    // bandwidth; limesdr stops after the same rate attempt, the paths not running.
    // Comparators failing at cap code 150 fail every 1 MS/s CGEN tuning, not a 1 GHz LO.
    let board = || SimBoard::new().with_lms7_comparator_failure(150);
    let (mut rate_only, rate_sim) = lime(board());
    let opened = rate_sim.board().trace().len();
    assert!(matches!(
        rate_only.set_sample_rate(1_000_000, 1_000_000),
        Err(Error::ClockUnlocked)
    ));
    let rate_attempt = since(&rate_sim, opened);

    let (mut device, sim) = lime(board());
    let opened = sim.board().trace().len();
    assert!(matches!(
        device.prepare_streaming(),
        Err(Error::NoSampleRate)
    ));
    assert_eq!(
        since(&sim, opened),
        rate_attempt,
        "the default rate's attempt, then nothing streamed up"
    );
    device.set_rx_frequency(1_000_000_000).expect("RX tuned");
    device.set_tx_frequency(1_000_000_000).expect("TX tuned");
    assert!(
        !since(&sim, opened)
            .iter()
            .any(|op| matches!(op, Op::FpgaWrite { addr, .. } if *addr == RF_SWITCHES)),
        "no path runs, so no tuning moves an antenna switch"
    );
}

#[test]
fn the_antenna_switches_move_only_once_streaming_is_prepared() {
    let (mut device, sim) = lime(SimBoard::new().with_fpga_reg(RF_SWITCHES, 0x0000));
    device.set_sample_rate(1_000_000, 1_000_000).expect("rate");
    device.set_rx_frequency(3_000_000_000).expect("RX tuned");
    device.set_tx_frequency(3_000_000_000).expect("TX tuned");
    assert_eq!(
        sim.board().fpga_reg(RF_SWITCHES),
        0,
        "not running: the bands are not selected"
    );
    device.prepare_streaming().expect("prepared");
    assert_eq!(
        sim.board().fpga_reg(RF_SWITCHES),
        1 << 12 | 1 << 8,
        "above 1.7 GHz the RX high band, above 2 GHz TX band 1: switch 1 each"
    );
}
