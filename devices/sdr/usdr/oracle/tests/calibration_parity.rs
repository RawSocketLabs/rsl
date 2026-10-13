//! rsl-usdr's LO-leakage and IQ-imbalance calibration against libusdr's `usdr_calibrate`
//! (`/dm/sdr/0/calibrate`), operation for operation, on fresh boards with the sim's RF
//! loopback (`rsl_usdr_sim::Loopback`): a synthetic LO leakage and TX and RX IQ
//! imbalances, measured through the RX chain's DC estimator. Every measurement is a
//! register read, so equal traces mean the searches probed the same points, saw the same
//! costs and settled on the same corrections; the TX LO result is also compared with
//! libusdr's result array.
//!
//! libusdr's array carries no IQ results (it repeats the LO search's record, or reads one
//! never written), so the IQ searches are compared through the registers alone.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use std::ops::Range;

use rsl_usdr::Error;
use rsl_usdr::{Calibration, CalibrationReport, Device, Feedback, IqCalibration, RxGain, TxGain};
use rsl_usdr_oracle::Oracle;
use rsl_usdr_sim::{BoardRevision, Imbalance, Loopback, Op, PllLock, SimBoard};
use sim_bus::SimBus;

/// libusdr's `USDR_CAL_TXLO`.
const TXLO: u32 = 2;
/// libusdr's `USDR_CAL_RXIQIMB`.
const RXIQIMB: u32 = 4;
/// libusdr's `USDR_CAL_TXIQIMB`.
const TXIQIMB: u32 = 8;
/// libusdr's `USDR_CAL_DUAL_RXLO`: TXVGA1's DC shift first.
const DUAL: u32 = 128;
/// libusdr's `USDR_CAL_EXT_FB`.
const EXTERNAL: u32 = 256;
/// libusdr's `USDR_CAL_COARSE_1`.
const COARSE_1: u32 = 512;
/// libusdr's `USDR_CAL_COARSE_2`.
const COARSE_2: u32 = 1024;

/// The sample rate every scenario sets: decimation 4, a 40 MHz converter clock.
const RATE: u32 = 10_000_000;

/// One call before the calibration, made the same way on both sides.
#[derive(Clone, Copy, Debug)]
enum Setup {
    /// `set_rx_sample_rate`.
    Rate(u32),
    /// `set_rx_frequency`.
    RxFrequency(u32),
    /// `set_tx_frequency`.
    TxFrequency(u32),
    /// `set_rx_gain(RxGain::Vga1)`, libusdr's `rx/gain/vga`.
    RxVga1(i32),
    /// `set_rx_gain(RxGain::Auto)`, libusdr's `rx/gain/auto`.
    RxAuto(i32),
    /// `set_tx_gain(TxGain::Vga1)`, libusdr's `tx/gain/vga1`.
    TxVga1(i32),
    /// `set_rx_bandwidth`.
    RxBandwidth(u32),
    /// `set_tx_bandwidth`.
    TxBandwidth(u32),
}

/// `db` as libusdr's numeric controls carry it: the 64-bit two's complement, which the
/// handler truncates to `int`.
fn gain_value(db: i32) -> u64 {
    u64::from_ne_bytes(i64::from(db).to_ne_bytes())
}

impl Setup {
    /// Through libusdr.
    fn on_libusdr(self, oracle: &mut Oracle) {
        match self {
            Self::Rate(rate) => oracle.set_rx_rate(rate),
            Self::RxFrequency(hz) => oracle.set_rx_frequency(hz),
            Self::TxFrequency(hz) => oracle.set_tx_frequency(hz),
            // SAFETY: the gain handlers read their value as a number.
            Self::RxVga1(db) => unsafe {
                oracle.set_uint(c"/dm/sdr/0/rx/gain/vga", gain_value(db))
            },
            // SAFETY: as above.
            Self::RxAuto(db) => unsafe {
                oracle.set_uint(c"/dm/sdr/0/rx/gain/auto", gain_value(db))
            },
            // SAFETY: as above.
            Self::TxVga1(db) => unsafe {
                oracle.set_uint(c"/dm/sdr/0/tx/gain/vga1", gain_value(db))
            },
            Self::RxBandwidth(hz) => oracle.set_rx_bandwidth(hz),
            Self::TxBandwidth(hz) => oracle.set_tx_bandwidth(hz),
        }
        .unwrap_or_else(|errno| panic!("libusdr {self:?}: {errno}"));
    }

    /// Through rsl-usdr.
    fn on_rsl(self, device: &mut Device) {
        match self {
            Self::Rate(rate) => device.set_rx_sample_rate(rate),
            Self::RxFrequency(hz) => device.set_rx_frequency(hz),
            Self::TxFrequency(hz) => device.set_tx_frequency(hz),
            Self::RxVga1(db) => device.set_rx_gain(RxGain::Vga1(db)),
            Self::RxAuto(db) => device.set_rx_gain(RxGain::Auto(db)),
            Self::TxVga1(db) => device.set_tx_gain(TxGain::Vga1(db)),
            Self::RxBandwidth(hz) => device.set_rx_bandwidth(hz),
            Self::TxBandwidth(hz) => device.set_tx_bandwidth(hz),
        }
        .unwrap_or_else(|error| panic!("rsl-usdr {self:?}: {error}"));
    }
}

/// The loopback the scenarios calibrate against: leakage off the integer grid, a few
/// percent and degrees of imbalance either way, unity gain.
fn loopback() -> Loopback {
    Loopback {
        converter_clock_hz: 4 * RATE,
        tx_lo_leakage: [-310.4, 205.7],
        tx_imbalance: Imbalance {
            gain: -0.02,
            phase: (-1.5_f64).to_radians(),
        },
        rx_imbalance: Imbalance {
            gain: 0.03,
            phase: 2_f64.to_radians(),
        },
        gain: 1.0,
        external: false,
    }
}

/// A revision-3 board with `loopback`.
fn board(loopback: Loopback) -> SimBoard {
    SimBoard::new(BoardRevision::Rev3).with_loopback(loopback)
}

/// The calls of a scenario where both chains are tuned to 1 GHz.
const BOTH_TUNED: [Setup; 3] = [
    Setup::Rate(RATE),
    Setup::RxFrequency(1_000_000_000),
    Setup::TxFrequency(1_000_000_000),
];

/// What a scenario left: libusdr's result array and board, and rsl-usdr's report and
/// board.
struct Outcome {
    /// libusdr's `cal_data`.
    expected: [i32; 8],
    /// rsl-usdr's report.
    report: CalibrationReport,
    /// The board libusdr drove.
    libusdr: SimBoard,
    /// The board rsl-usdr drove.
    rsl: SimBus,
}

/// Runs `setup` then a calibration with `flags` (libusdr) and `calibration` (rsl-usdr) on
/// fresh boards from `make`, and compares the operations from the first setup call on.
fn assert_calibration_matches(
    make: impl Fn() -> SimBoard,
    setup: &[Setup],
    flags: u32,
    calibration: Calibration,
) -> Outcome {
    let mut oracle = Oracle::open(make()).expect("libusdr opens the board");
    let start = oracle.trace_len();
    for &call in setup {
        call.on_libusdr(&mut oracle);
    }
    let calibrating = oracle.trace_len();
    let expected = oracle
        .calibrate(flags)
        .unwrap_or_else(|errno| panic!("libusdr calibration failed: {errno}"));
    let end = oracle.trace_len();
    let libusdr = oracle.close();

    let rsl = SimBus::new(make());
    let mut device = Device::with_bus(rsl.clone()).expect("rsl-usdr powers up the board");
    let rsl_start = rsl.board().trace().len();
    for &call in setup {
        call.on_rsl(&mut device);
    }
    let rsl_calibrating = rsl.board().trace().len();
    let report = device
        .calibrate(calibration)
        .unwrap_or_else(|error| panic!("rsl-usdr calibration failed: {error}"));
    let rsl_end = rsl.board().trace().len();
    // Keep the device, and so the powered board, until the comparison is done.
    {
        let board = rsl.board();
        let window = |trace: &[Op], range: Range<usize>| trace[range].to_vec();
        assert_same_ops(
            &window(libusdr.trace(), start..calibrating),
            &window(board.trace(), rsl_start..rsl_calibrating),
            "setup",
        );
        assert_same_ops(
            &window(libusdr.trace(), calibrating..end),
            &window(board.trace(), rsl_calibrating..rsl_end),
            "calibration",
        );
    }
    drop(device);
    Outcome {
        expected,
        report,
        libusdr,
        rsl,
    }
}

/// Panics at the first operation where `actual` departs from `expected`.
fn assert_same_ops(expected: &[Op], actual: &[Op], context: &str) {
    if let Some(i) = expected.iter().zip(actual).position(|(e, a)| e != a) {
        panic!(
            "{context}: first difference at op {i} of {}: libusdr {:x?}, rsl-usdr {:x?}",
            expected.len(),
            expected[i],
            actual[i]
        );
    }
    assert_eq!(
        expected.len(),
        actual.len(),
        "{context}: same prefix, different lengths"
    );
}

/// libusdr's TX LO result is its search's X and Y: Q then I, as it stores them
/// crosswise. rsl-usdr reports them by name.
fn assert_tx_lo_matches(outcome: &Outcome) {
    let leakage = outcome.report.tx_lo_leakage.expect("TX LO calibrated");
    assert_eq!(
        [i32::from(leakage.q.value()), i32::from(leakage.i.value())],
        outcome.expected[2..4],
        "libusdr's result array"
    );
}

/// With both chains tuned, the TX LO search nulls the leakage: the same probes, the same
/// result, close to the leakage it cancels.
#[test]
fn tx_lo_leakage_calibration_matches_libusdr() {
    let outcome = assert_calibration_matches(
        || board(loopback()),
        &BOTH_TUNED,
        TXLO,
        Calibration {
            tx_lo_leakage: true,
            ..Calibration::default()
        },
    );
    assert_tx_lo_matches(&outcome);
    let leakage = outcome.report.tx_lo_leakage.expect("TX LO calibrated");
    assert!(
        (i32::from(leakage.i.value()) - 317).abs() <= 1
            && (i32::from(leakage.q.value()) + 197).abs() <= 1,
        "the offset cancels the leakage through the TX imbalance, (317, -197): {leakage:?}"
    );
    assert!(outcome.report.tx_iq_imbalance.is_none() && outcome.report.rx_iq_imbalance.is_none());
}

/// All three, in libusdr's order: RX IQ with the TX LO moved above the RX LO, then the
/// TX LO and TX IQ searches with the RX LO moved below, then everything restored.
#[test]
fn a_full_calibration_matches_libusdr() {
    let outcome = assert_calibration_matches(
        || board(loopback()),
        &BOTH_TUNED,
        RXIQIMB | TXLO | TXIQIMB,
        Calibration {
            tx_lo_leakage: true,
            tx_iq_imbalance: true,
            rx_iq_imbalance: true,
            ..Calibration::default()
        },
    );
    assert_tx_lo_matches(&outcome);
    // The corrections the model needs: half the quadrature error against each axis
    // (1000 phase steps per degree) and the I or Q gain scaled to match the other.
    for (iq, phase, amplitude) in [
        (outcome.report.rx_iq_imbalance, -1000, 7635),
        (outcome.report.tx_iq_imbalance, 750, -5243),
    ] {
        let Some(IqCalibration::Corrected(found)) = iq else {
            panic!("the IQ search ran: {iq:?}");
        };
        assert!(
            (found.phase - phase).abs() <= phase.abs() / 20
                && (found.amplitude - amplitude).abs() <= amplitude.abs() / 20,
            "within 5% of ({phase}, {amplitude}): {found:?}"
        );
    }
    // Both left the LMS6002D alike, the TX PLL back on 1 GHz and the loopback off.
    for addr in 0..0x80 {
        assert_eq!(
            outcome.libusdr.lms_reg(addr),
            outcome.rsl.board().lms_reg(addr),
            "LMS6002D register {addr:#04x}"
        );
    }
}

/// With only the transmitter tuned, the TX calibrations power the receiver, tune it to
/// 320 MHz and DC-calibrate it first, then turn its PLL off and power it down again.
#[test]
fn tx_calibration_of_an_untuned_receiver_matches_libusdr() {
    let outcome = assert_calibration_matches(
        || board(loopback()),
        &[Setup::Rate(RATE), Setup::TxFrequency(2_400_000_000)],
        TXLO | TXIQIMB,
        Calibration {
            tx_lo_leakage: true,
            tx_iq_imbalance: true,
            ..Calibration::default()
        },
    );
    assert_tx_lo_matches(&outcome);
}

/// With only the receiver tuned, the RX IQ search powers the transmitter, tunes it for
/// the tone, then turns its PLL off and powers it down; the TX calibrations, asked for,
/// are skipped.
#[test]
fn rx_calibration_of_an_untuned_transmitter_matches_libusdr() {
    let outcome = assert_calibration_matches(
        || board(loopback()),
        &[Setup::Rate(RATE), Setup::RxFrequency(900_000_000)],
        RXIQIMB | TXLO,
        Calibration {
            rx_iq_imbalance: true,
            tx_lo_leakage: true,
            ..Calibration::default()
        },
    );
    assert!(outcome.report.tx_lo_leakage.is_none());
    assert!(matches!(
        outcome.report.rx_iq_imbalance,
        Some(IqCalibration::Corrected(_))
    ));
}

/// A loop too weak to measure: each IQ search raises the tone to full power, then the
/// other chain's gain step by step to its maximum (RXVGA1 from 20 dB for the TX search),
/// and gives up without searching, which neither driver counts as a failure.
#[test]
fn a_signal_too_low_to_measure_matches_libusdr() {
    let outcome = assert_calibration_matches(
        || {
            board(Loopback {
                gain: 1e-4,
                ..loopback()
            })
        },
        &[
            Setup::Rate(RATE),
            Setup::RxFrequency(1_000_000_000),
            Setup::TxFrequency(1_000_000_000),
            Setup::RxVga1(20),
        ],
        RXIQIMB | TXIQIMB,
        Calibration {
            tx_iq_imbalance: true,
            rx_iq_imbalance: true,
            ..Calibration::default()
        },
    );
    assert_eq!(
        outcome.report.rx_iq_imbalance,
        Some(IqCalibration::SignalTooLow)
    );
    assert_eq!(
        outcome.report.tx_iq_imbalance,
        Some(IqCalibration::SignalTooLow)
    );
}

/// The options: TXVGA1's DC shift searched first (both searches then within -128 to 127,
/// as libusdr leaves the limits narrowed) and an external loop that leaves the loopback
/// switch alone. libusdr's coarse flags change nothing (its setup zeroes the mode), so
/// rsl-usdr does not offer them: a full calibration matches libusdr's with them.
#[test]
fn calibration_options_match_libusdr() {
    let external = || {
        board(Loopback {
            external: true,
            ..loopback()
        })
    };
    let outcome = assert_calibration_matches(
        external,
        &BOTH_TUNED,
        TXLO | DUAL | EXTERNAL,
        Calibration {
            tx_lo_leakage: true,
            feedback: Feedback::External,
            analog_lo_first: true,
            ..Calibration::default()
        },
    );
    assert_tx_lo_matches(&outcome);
    let outcome = assert_calibration_matches(
        || board(loopback()),
        &BOTH_TUNED,
        TXLO | COARSE_1 | COARSE_2,
        Calibration {
            tx_lo_leakage: true,
            ..Calibration::default()
        },
    );
    assert_tx_lo_matches(&outcome);
}

/// A transmitter on the low-LO fallback (its PLL locks only from 245 MHz, so 240 MHz runs
/// 5 MHz off on the NCOs): libusdr measures 0.2 of the rate further up, retunes the TX PLL
/// there and back, and the NCOs return to the offset.
#[test]
fn tx_calibration_on_an_nco_offset_matches_libusdr() {
    let make = || {
        board(Loopback {
            converter_clock_hz: 32_000_000,
            ..loopback()
        })
        .with_tx_pll_lock(PllLock {
            unlocked_below_hz: 245_000_000,
            ..PllLock::default()
        })
    };
    let outcome = assert_calibration_matches(
        make,
        &[
            Setup::Rate(1_000_000),
            Setup::RxFrequency(1_000_000_000),
            Setup::TxFrequency(240_000_000),
        ],
        TXLO | TXIQIMB,
        Calibration {
            tx_lo_leakage: true,
            tx_iq_imbalance: true,
            ..Calibration::default()
        },
    );
    assert_tx_lo_matches(&outcome);
}

/// An RX PLL that cannot lock where the TX LO search measures stops both drivers there,
/// after the same operations and without restoring, as libusdr does.
#[test]
fn an_unreachable_measuring_frequency_fails_like_libusdr() {
    let make = || {
        board(loopback()).with_rx_pll_lock(PllLock {
            unlocked_below_hz: 2_450_000_000,
            ..PllLock::default()
        })
    };
    let setup = [
        Setup::Rate(RATE),
        Setup::RxFrequency(2_500_000_000),
        Setup::TxFrequency(2_450_000_000),
    ];
    let mut oracle = Oracle::open(make()).expect("libusdr opens the board");
    for &call in &setup {
        call.on_libusdr(&mut oracle);
    }
    let start = oracle.trace_len();
    assert!(oracle.calibrate(TXLO).is_err(), "libusdr fails");
    let end = oracle.trace_len();
    let libusdr = oracle.close();

    let rsl = SimBus::new(make());
    let mut device = Device::with_bus(rsl.clone()).expect("rsl-usdr powers up the board");
    for &call in &setup {
        call.on_rsl(&mut device);
    }
    let rsl_start = rsl.board().trace().len();
    let result = device.calibrate(Calibration {
        tx_lo_leakage: true,
        ..Calibration::default()
    });
    assert!(
        matches!(result, Err(Error::PllUnlocked(_))),
        "rsl-usdr fails alike: {result:?}"
    );
    let board = rsl.board();
    assert_same_ops(
        &libusdr.trace()[start..end],
        &board.trace()[rsl_start..],
        "failed calibration",
    );
}

/// The caller's settings the calibration reads and restores: fixed RX and TX filters,
/// RXVGA1 from the automatic split (the TX IQ search raises it from there and puts it
/// back) and TXVGA1 set by hand (the TX IQ search runs at -7 dB and returns to it), with
/// every search over an external loop.
#[test]
fn a_calibration_restores_the_callers_settings_like_libusdr() {
    let outcome = assert_calibration_matches(
        || {
            board(Loopback {
                external: true,
                ..loopback()
            })
        },
        &[
            Setup::Rate(RATE),
            Setup::RxFrequency(1_000_000_000),
            Setup::TxFrequency(1_000_000_000),
            Setup::RxBandwidth(5_000_000),
            Setup::TxBandwidth(3_000_000),
            Setup::RxAuto(40),
            Setup::TxVga1(-20),
        ],
        RXIQIMB | TXLO | TXIQIMB | EXTERNAL,
        Calibration {
            tx_lo_leakage: true,
            tx_iq_imbalance: true,
            rx_iq_imbalance: true,
            feedback: Feedback::External,
            ..Calibration::default()
        },
    );
    assert_tx_lo_matches(&outcome);
}
