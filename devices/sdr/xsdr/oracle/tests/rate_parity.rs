//! rsl-xsdr's sample rate, LO tuning, gains and bandwidths against libusdr's, operation for
//! operation, on fresh simulated XSDRs: everything between the open and the close.
//!
//! The boards report gateware with a TX MMCM (HWID PHY bit 0x20) unless a scenario says
//! otherwise: libusdr's rate path calibrates the `LimeLight` link only then. The sim's RX
//! PHY reads an error-free link and its LMS7002M answers the VCO comparators from the
//! 26 MHz reference (`rsl-xsdr-sim`, `rsl-lms7002m-sim`).
//!
//! Whitelisted divergence: gateware with an RX or shared MMCM or a DUC/DDC is refused by
//! rsl-xsdr before any bus access, where libusdr configures it
//! (`gateware_rsl_xsdr_does_not_port_is_refused_before_any_access`).

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use rsl_xsdr::{Device, Direction, Error, Gain};
use rsl_xsdr_oracle::Oracle;
use rsl_xsdr_sim::{Op, SimBoard, Variant};
use sim_bus::SimBus;

/// `EINVAL`.
const EINVAL: i32 = 22;
/// `ERANGE`.
const ERANGE: i32 = 34;
/// `EIO`.
const EIO: i32 = 5;
/// An XSDR with a TX MMCM.
const TX_MMCM: u32 = 0x0030_0020;

/// One device-manager call, as libusdr's path and rsl-xsdr's method.
#[derive(Clone, Copy, Debug)]
enum Call {
    /// `/dm/power/en` = 1.
    PowerUp,
    /// `/dm/rate/master`: both directions at one rate.
    Rate(u32),
    /// `/dm/rate/rxtxadcdac`: RX and TX rates.
    Rates(u32, u32),
    /// `/dm/sdr/0/rx/frequency`.
    Rx(u32),
    /// `/dm/sdr/0/tx/frequency`.
    Tx(u32),
    /// A gain path and value.
    Gain(Gain, i32),
    /// A bandwidth path and value.
    Bandwidth(Direction, u32),
}

/// libusdr's device-manager path for a gain.
fn gain_path(gain: Gain) -> &'static core::ffi::CStr {
    match gain {
        Gain::RxLna => c"/dm/sdr/0/rx/gain/lna",
        Gain::RxTia => c"/dm/sdr/0/rx/gain/vga",
        Gain::RxPga => c"/dm/sdr/0/rx/gain/pga",
        Gain::RxLoopback => c"/dm/sdr/0/rx/gain/lb",
        Gain::TxPad => c"/dm/sdr/0/tx/gain",
        Gain::TxLoopback => c"/dm/sdr/0/tx/gain/lb",
        Gain::TxPga => c"/dm/sdr/0/tx/gain/vga1",
    }
}

/// libusdr's results and the operations between its open and close.
fn libusdr(hwid: u32, calls: &[Call]) -> (Vec<i32>, Vec<Op>) {
    let mut oracle = Oracle::open(SimBoard::new(Variant::Xsdr).with_hwid(hwid))
        .expect("libusdr opens the board");
    let opened = oracle.trace_len();
    let results = calls
        .iter()
        .map(|&call| {
            let result = match call {
                Call::PowerUp => oracle.power_up(),
                Call::Rate(rate) => oracle.set_uint(c"/dm/rate/master", rate.into()),
                Call::Rates(rx, tx) => oracle.set_rates(rx, tx),
                Call::Rx(hz) => oracle.set_uint(c"/dm/sdr/0/rx/frequency", hz.into()),
                Call::Tx(hz) => oracle.set_uint(c"/dm/sdr/0/tx/frequency", hz.into()),
                Call::Gain(gain, value) => {
                    // The handlers take the u64 as an `int`: two's complement.
                    oracle.set_uint(
                        gain_path(gain),
                        u64::from_ne_bytes(i64::from(value).to_ne_bytes()),
                    )
                }
                Call::Bandwidth(Direction::Rx, hz) => {
                    oracle.set_uint(c"/dm/sdr/0/rx/bandwidth", hz.into())
                }
                Call::Bandwidth(_, hz) => oracle.set_uint(c"/dm/sdr/0/tx/bandwidth", hz.into()),
            };
            result.err().unwrap_or(0)
        })
        .collect();
    let used = oracle.trace_len();
    let board = oracle.close();
    (results, board.trace()[opened..used].to_vec())
}

/// rsl-xsdr's results, as libusdr's errnos, and its operations after the open.
fn rsl(hwid: u32, calls: &[Call]) -> (Vec<i32>, Vec<Op>) {
    let sim = SimBus::new(SimBoard::new(Variant::Xsdr).with_hwid(hwid));
    let mut device = Device::open(sim.clone()).expect("rsl-xsdr opens the XSDR");
    let opened = sim.board().trace().len();
    let results = calls
        .iter()
        .map(|&call| {
            let result = match call {
                Call::PowerUp => device.power_up(),
                Call::Rate(rate) => device.set_sample_rate(rate, rate),
                Call::Rates(rx, tx) => device.set_sample_rate(rx, tx),
                Call::Rx(hz) => device.set_rx_frequency(hz),
                Call::Tx(hz) => device.set_tx_frequency(hz),
                Call::Gain(gain, value) => device.set_gain(gain, value).map(|_| ()),
                Call::Bandwidth(direction, hz) => device.set_bandwidth(direction, hz),
            };
            errno(result)
        })
        .collect();
    let trace = sim.board().trace()[opened..].to_vec();
    (results, trace)
}

/// The errno libusdr returns where rsl-xsdr returns `result`.
fn errno(result: Result<(), Error>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(Error::InvalidRate(0) | Error::UndeliverableRate | Error::NoChannel) => -EINVAL,
        Err(
            Error::InvalidRate(_)
            | Error::ClockUnlocked
            | Error::LinkCalibration
            | Error::TuneOutOfRange(_),
        ) => -ERANGE,
        Err(Error::VcoComparatorFailed) => -EIO,
        Err(other) => panic!("no libusdr counterpart: {other}"),
    }
}

/// Panics, naming `scenario`, where rsl-xsdr leaves libusdr.
fn assert_same(hwid: u32, calls: &[Call], scenario: &str) -> Vec<i32> {
    let (expected, actual) = (libusdr(hwid, calls), rsl(hwid, calls));
    assert_eq!(actual.0, expected.0, "{scenario}: results");
    if let Some(at) =
        (0..expected.1.len().max(actual.1.len())).find(|&i| expected.1.get(i) != actual.1.get(i))
    {
        panic!(
            "{scenario}: rsl-xsdr diverges at op {at} of {}: libusdr {}, rsl-xsdr {}",
            expected.1.len(),
            expected.1.get(at).map_or("(end)".into(), Op::to_string),
            actual.1.get(at).map_or("(end)".into(), Op::to_string),
        );
    }
    expected.0
}

#[test]
fn a_session_of_rates_tunings_gains_and_bandwidths_matches_libusdr() {
    let results = assert_same(
        TX_MMCM,
        &[
            Call::PowerUp,
            Call::Rx(900_000_000),
            Call::Rate(1_000_000),
            Call::Rate(10_000_000),
            Call::Rx(1_000_000_000),
            Call::Rx(1_000_000_000),
            Call::Rx(2_400_000_000),
            Call::Tx(1_500_000_000),
            Call::Rx(4_100_000_000),
            Call::Rx(0),
            Call::Rx(3_500_000_000),
            Call::Gain(Gain::RxLna, 15),
            Call::Gain(Gain::RxLna, 30),
            Call::Gain(Gain::RxTia, -3),
            Call::Gain(Gain::RxPga, 12),
            Call::Gain(Gain::RxPga, 40),
            Call::Gain(Gain::RxLoopback, -20),
            Call::Gain(Gain::TxPad, -20),
            Call::Gain(Gain::TxPad, 5),
            Call::Gain(Gain::TxLoopback, -22),
            Call::Gain(Gain::TxPga, 13),
            Call::Gain(Gain::TxPga, -100),
            Call::Bandwidth(Direction::Rx, 5_000_000),
            Call::Bandwidth(Direction::Tx, 15_000_000),
            Call::Bandwidth(Direction::Rx, 30_000_000),
            Call::Bandwidth(Direction::Tx, 0),
            Call::Rate(30_720_000),
        ],
        "session",
    );
    assert!(results.contains(&-ERANGE), "the 4.1 GHz LO is refused");
    assert_eq!(
        results.iter().filter(|&&r| r == 0).count(),
        results.len() - 1
    );
}

#[test]
fn each_rate_matches_libusdr_on_a_fresh_board() {
    for rate in [
        90_000,
        1_000_000,
        2_000_000,
        7_680_000,
        15_360_000,
        30_720_000,
        61_440_000,
        80_000_000,
        122_880_000,
        195_000_000,
    ] {
        let results = assert_same(
            TX_MMCM,
            &[Call::PowerUp, Call::Rate(rate)],
            &format!("{rate} S/s"),
        );
        // At 195 MS/s CGEN needs a VCO above any the sim's model reaches.
        let expected = if rate == 195_000_000 { -ERANGE } else { 0 };
        assert_eq!(results[1], expected, "{rate} S/s: libusdr's outcome");
    }
}

#[test]
fn unequal_and_one_sided_rates_match_libusdr() {
    for (rx, tx) in [
        (1_000_000, 3_000_000),
        (0, 5_000_000),
        (5_000_000, 0),
        (40_000_000, 20_000_000),
    ] {
        assert_same(
            TX_MMCM,
            &[Call::PowerUp, Call::Rates(rx, tx)],
            &format!("RX {rx}, TX {tx}"),
        );
    }
    let results = assert_same(
        TX_MMCM,
        &[Call::PowerUp, Call::Rates(1_000_000, 3_000_000)],
        "undeliverable",
    );
    assert_eq!(results[1], -EINVAL, "RX would divide by 24");
}

#[test]
fn a_rate_before_power_up_powers_the_rfic_up_first_like_libusdr() {
    assert_same(TX_MMCM, &[Call::Rate(2_000_000)], "rate without power-up");
}

#[test]
fn rates_out_of_range_are_refused_before_any_access_like_libusdr() {
    let results = assert_same(
        TX_MMCM,
        &[
            Call::Rate(89_999),
            Call::Rate(195_000_001),
            Call::Rates(0, 0),
        ],
        "refused rates",
    );
    assert_eq!(results, [-ERANGE, -ERANGE, -EINVAL]);
}

#[test]
fn gateware_without_a_tx_mmcm_fails_the_link_calibration_like_libusdr() {
    let results = assert_same(
        0x0030_0000,
        &[Call::PowerUp, Call::Rate(1_000_000)],
        "no TX MMCM",
    );
    assert_eq!(
        results[1], -ERANGE,
        "libusdr's RX badness is never measured"
    );
}

#[test]
fn rx_on_lml_port_2_matches_libusdr() {
    assert_same(
        0x0030_0060,
        &[
            Call::PowerUp,
            Call::Rate(10_000_000),
            Call::Rx(2_000_000_000),
        ],
        "LML2 is RX",
    );
}

#[test]
fn gateware_rsl_xsdr_does_not_port_is_refused_before_any_access() {
    for phy in [0x10, 0x01, 0x04] {
        let sim = SimBus::new(SimBoard::new(Variant::Xsdr).with_hwid(TX_MMCM | phy));
        let mut device = Device::open(sim.clone()).expect("opens");
        device.power_up().expect("powers up");
        let before = sim.board().trace().len();
        assert!(matches!(
            device.set_sample_rate(1_000_000, 1_000_000),
            Err(Error::UnsupportedGateware(_))
        ));
        assert_eq!(
            sim.board().trace().len(),
            before,
            "PHY {phy:#04x}: nothing driven"
        );
    }
}
