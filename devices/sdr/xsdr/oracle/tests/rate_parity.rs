//! xsdr's sample rate, LO tuning, gains and bandwidths against libusdr's, operation for
//! operation, on fresh simulated XSDRs: everything between the open and the close.
//!
//! The boards report gateware with a TX MMCM (HWID PHY bit 0x20) unless a scenario says
//! otherwise: libusdr's rate path calibrates the `LimeLight` link only then. The sim's RX
//! PHY reads an error-free link and its LMS7002M answers the VCO comparators from the
//! 26 MHz reference (`xsdr-sim`, `lms7002m-sim`).
//!
//! Whitelisted divergence: gateware with an RX or shared MMCM or a DUC/DDC is refused by
//! xsdr before any bus access, where libusdr configures it
//! (`gateware_xsdr_does_not_port_is_refused_before_any_access`).

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use sim_bus::SimBus;
use xsdr::{Device, Direction, Error, Gain};
use xsdr_oracle::Oracle;
use xsdr_sim::{Op, SimBoard, Variant};

/// `EINVAL`.
const EINVAL: i32 = 22;
/// `ERANGE`.
const ERANGE: i32 = 34;
/// `EIO`.
const EIO: i32 = 5;
/// An XSDR with a TX MMCM.
const TX_MMCM: u32 = 0x0030_0020;

/// One device-manager call, as libusdr's path and xsdr's method.
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

/// Sets a numeric device-manager path.
fn set_uint(oracle: &mut Oracle, path: &core::ffi::CStr, value: u64) -> Result<(), i32> {
    // SAFETY: every path these tests set (rate master, frequencies, gains, bandwidths)
    // takes its value as a number, not a pointer.
    unsafe { oracle.set_uint(path, value) }
}

/// libusdr's results and the operations between its open and close.
fn libusdr(board: SimBoard, calls: &[Call]) -> (Vec<i32>, Vec<Op>) {
    let mut oracle = Oracle::open(board).expect("libusdr opens the board");
    let opened = oracle.trace_len();
    let results = calls
        .iter()
        .map(|&call| {
            let result = match call {
                Call::PowerUp => oracle.power_up(),
                Call::Rate(rate) => set_uint(&mut oracle, c"/dm/rate/master", rate.into()),
                Call::Rates(rx, tx) => oracle.set_rates(rx, tx),
                Call::Rx(hz) => set_uint(&mut oracle, c"/dm/sdr/0/rx/frequency", hz.into()),
                Call::Tx(hz) => set_uint(&mut oracle, c"/dm/sdr/0/tx/frequency", hz.into()),
                Call::Gain(gain, value) => {
                    // The handlers take the u64 as an `int`: two's complement.
                    set_uint(
                        &mut oracle,
                        gain_path(gain),
                        u64::from_ne_bytes(i64::from(value).to_ne_bytes()),
                    )
                }
                Call::Bandwidth(Direction::Rx, hz) => {
                    set_uint(&mut oracle, c"/dm/sdr/0/rx/bandwidth", hz.into())
                }
                Call::Bandwidth(_, hz) => {
                    set_uint(&mut oracle, c"/dm/sdr/0/tx/bandwidth", hz.into())
                }
            };
            result.err().unwrap_or(0)
        })
        .collect();
    let used = oracle.trace_len();
    let board = oracle.close();
    (results, board.trace()[opened..used].to_vec())
}

/// xsdr's results, as libusdr's errnos, and its operations after the open.
fn rsl(board: SimBoard, calls: &[Call]) -> (Vec<i32>, Vec<Op>) {
    let sim = SimBus::new(board);
    let mut device = Device::open(sim.clone()).expect("xsdr opens the XSDR");
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

/// The errno libusdr returns where xsdr returns `result`.
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

/// Panics, naming `scenario`, where xsdr leaves libusdr.
fn assert_same(hwid: u32, calls: &[Call], scenario: &str) -> Vec<i32> {
    assert_same_on(
        &|| SimBoard::new(Variant::Xsdr).with_hwid(hwid),
        calls,
        scenario,
    )
}

/// [`assert_same`] on boards `board` builds.
fn assert_same_on(board: &dyn Fn() -> SimBoard, calls: &[Call], scenario: &str) -> Vec<i32> {
    let (expected, actual) = (libusdr(board(), calls), rsl(board(), calls));
    assert_eq!(actual.0, expected.0, "{scenario}: results");
    if let Some(at) =
        (0..expected.1.len().max(actual.1.len())).find(|&i| expected.1.get(i) != actual.1.get(i))
    {
        panic!(
            "{scenario}: xsdr diverges at op {at} of {}: libusdr {}, xsdr {}",
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
fn gateware_xsdr_does_not_port_is_refused_before_any_access() {
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

#[test]
fn a_link_dirty_for_three_phases_is_swept_past_like_libusdr() {
    // Phases 1 to 3 read one error on their first counter; the rest read clean.
    let dirty = [1, 0, 0, 0].repeat(3);
    let results = assert_same_on(
        &|| {
            SimBoard::new(Variant::Xsdr)
                .with_hwid(TX_MMCM)
                .with_rx_phy_reads(dirty.clone(), 0)
        },
        &[Call::PowerUp, Call::Rate(1_000_000)],
        "three dirty phases",
    );
    assert_eq!(results[1], 0, "a clean band follows");
}

#[test]
fn a_link_dirty_at_every_phase_retries_five_times_like_libusdr() {
    // One error per counter scores 4 per check, rescaled to 40: a warning, not a failure.
    // Eleven per counter scores 4840: the rate fails.
    for (errors, expected) in [(1, 0), (11, -ERANGE)] {
        let results = assert_same_on(
            &|| {
                SimBoard::new(Variant::Xsdr)
                    .with_hwid(TX_MMCM)
                    .with_rx_phy_reads([], errors)
            },
            &[Call::PowerUp, Call::Rate(1_000_000)],
            &format!("{errors} errors per counter"),
        );
        assert_eq!(results[1], expected, "{errors} errors per counter");
    }
}

#[test]
fn a_failing_vco_comparator_fails_tuning_and_the_rate_like_libusdr() {
    let results = assert_same_on(
        &|| {
            SimBoard::new(Variant::Xsdr)
                .with_hwid(TX_MMCM)
                .with_lms_comparator_failure(128)
        },
        &[
            Call::PowerUp,
            Call::Rx(1_000_000_000),
            Call::Rate(1_000_000),
        ],
        "comparator failure",
    );
    assert_eq!(
        results[1..],
        [-EIO, -ERANGE],
        "SXX fails with -EIO; every CGEN attempt fails, so the rate -ERANGE"
    );
}

#[test]
fn an_xtrx_raises_only_its_io_supply_above_50_and_60_ms_like_libusdr() {
    // `_xsdr_calibrate_lml` on `!new_rev`: 1.96 V above 50 MS/s, 2.15 V above 60 MS/s, no
    // core supply; the XTRX's bands for the LOs.
    for rate in [40_000_000, 55_000_000, 65_000_000, 80_000_000] {
        assert_same_on(
            &|| SimBoard::new(Variant::Xtrx).with_hwid(0x002e_0020),
            &[
                Call::PowerUp,
                Call::Rate(rate),
                Call::Rx(1_000_000_000),
                Call::Tx(3_000_000_000),
            ],
            &format!("XTRX at {rate} S/s"),
        );
    }
}
