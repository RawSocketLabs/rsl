//! xsdr's RF calibration (`/dm/sdr/0/calibrate`, `xsdr_calibrate`) and DC readback
//! (`/dm/sdr/0/rx/dccorr` read, `xsdr_rxdccorr`) against libusdr's, operation for
//! operation, on fresh simulated XSDRs whose LMS7002M carries a synthetic RF loop
//! (`lms7002m-sim`'s model: offsets, leakage, imbalances) for the RX PHY's DC estimator to
//! measure. Everything between the open and the close is compared, with the calibrations'
//! LO results and the DC levels.
//!
//! The boards report gateware with a TX MMCM and a DUC/DDC, so libusdr's restore divides
//! by the DSP chains' factors without trapping; off DUC/DDC gateware it traps there
//! (SIGFPE), which xsdr does not, and the scenario compares up to the trap. A rate, both
//! LOs and a TX band come first, as the TX calibrations need the band's loopback.
//!
//! Whitelisted divergences: libusdr's I/Q slots of its results (`cal_data[4..8]`) hold the
//! last LO search's values or an uninitialised stack and are not compared; a calibration
//! before any sample rate, where libusdr divides by zero, is refused by xsdr and not run
//! in libusdr (`tests/calibrate.rs`).

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use core::ffi::CStr;

use sim_bus::SimBus;
use xsdr::{Calibrations, Device, Error, Gain, Mac, RfCalibration, TxChannels, TxRoute};
use xsdr_oracle::Oracle;
use xsdr_sim::{Imbalance, Loopback, Op, SimBoard, Variant};

/// `EAGAIN`.
const EAGAIN: i32 = 11;
/// `EINVAL`.
const EINVAL: i32 = 22;
/// `ERANGE`.
const ERANGE: i32 = 34;
/// `ENAVAIL`.
const ENAVAIL: i32 = 119;
/// An XSDR with a TX MMCM and a DUC/DDC.
const DUC_DDC: u32 = 0x0030_0024;
/// A gateware build libusdr streams TX from (`MINIM_FWID_COMPAT`).
const TX_GATEWARE: u32 = 0xd2b1_0c09;
/// An XSDR with a TX MMCM only.
const TX_MMCM: u32 = 0x0030_0020;

/// `XSDR_CAL_RXLO`.
const RX_LO: u64 = 1;
/// `XSDR_CAL_TXLO`.
const TX_LO: u64 = 2;
/// `XSDR_CAL_RXIQIMB`.
const RX_IQ: u64 = 4;
/// `XSDR_CAL_TXIQIMB`.
const TX_IQ: u64 = 8;
/// `XSDR_CAL_EXT_FB`.
const EXTERNAL: u64 = 256;
/// `XSDR_DONT_SETBACK`.
const KEEP: u64 = 65_536;
/// The handler's channel B bit.
const CHANNEL_B: u64 = 262_144;
/// Every calibration.
const ALL: u64 = RX_LO | TX_LO | RX_IQ | TX_IQ;

/// A loop whose every search has something to find.
const LOOP: Loopback = Loopback {
    rx_dc_offset: [12.3, -7.6],
    tx_lo_leakage: [83.4, -51.2],
    tx_imbalance: Imbalance {
        gain: 0.02,
        phase: 0.026,
    },
    rx_imbalance: Imbalance {
        gain: -0.015,
        phase: 0.017,
    },
    gain: 0.3,
    external: false,
};

/// One step of a scenario, as libusdr's path and value and xsdr's method.
#[derive(Clone, Copy, Debug)]
enum Call {
    /// `/dm/power/en` = 1.
    PowerUp,
    /// `/dm/rate/master`.
    Rate(u32),
    /// `/dm/sdr/0/rx/frequency`.
    Rx(u32),
    /// `/dm/sdr/0/tx/frequency`.
    Tx(u32),
    /// `/dm/sdr/0/tx/path`.
    TxPath(TxRoute),
    /// `/dm/sdr/0/tx/gain/vga1`.
    TxPga(i32),
    /// A TX stream on channel A, 64 samples per packet.
    CreateTx,
    /// `/dm/sdr/0/calibrate` with libusdr's flags.
    Calibrate(u64),
    /// `/dm/sdr/0/rx/dccorr` read.
    DcLevel,
}

/// What a step returned: an errno, a calibration's LO results or a DC level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    /// A failure's errno, or 0.
    Errno(i32),
    /// A calibration's RX LO and TX LO codes.
    Lo([i32; 4]),
    /// A DC level, in millidB.
    Level(i32),
}

/// The channel and calibrations libusdr's `flags` ask for.
fn calibrations(flags: u64) -> (Mac, Calibrations) {
    let channel = if flags & CHANNEL_B == 0 {
        Mac::A
    } else {
        Mac::B
    };
    let calibrations = Calibrations {
        rx_lo: flags & RX_LO != 0,
        tx_lo: flags & TX_LO != 0,
        rx_iq: flags & RX_IQ != 0,
        tx_iq: flags & TX_IQ != 0,
        external_loopback: flags & EXTERNAL != 0,
        keep_settings: flags & KEEP != 0,
    };
    (channel, calibrations)
}

/// libusdr's code for a TX path.
const fn tx_code(route: TxRoute) -> u64 {
    match route {
        TxRoute::Band1 => 6,
        TxRoute::Band2 => 7,
        TxRoute::W => 8,
        TxRoute::H => 9,
        TxRoute::Auto => 11,
    }
}

/// libusdr's outcomes, whether it trapped, and the operations between its open and close.
fn libusdr(board: SimBoard, calls: &[Call]) -> (Vec<Outcome>, bool, Vec<Op>) {
    let mut oracle = Oracle::open(board).expect("libusdr opens the board");
    let opened = oracle.trace_len();
    let mut outcomes = Vec::new();
    let mut trapped = false;
    for &call in calls {
        let set = |oracle: &mut Oracle, path: &CStr, value| {
            // SAFETY: every handler these paths reach takes its value as a number (the
            // calibration's reads a string only above 2^24, which no flag reaches).
            unsafe { oracle.set_uint_trapping(path, value) }
        };
        let result = match call {
            Call::PowerUp => oracle.power_up().map(|()| false),
            Call::Rate(rate) => set(&mut oracle, c"/dm/rate/master", rate.into()),
            Call::Rx(hz) => set(&mut oracle, c"/dm/sdr/0/rx/frequency", hz.into()),
            Call::Tx(hz) => set(&mut oracle, c"/dm/sdr/0/tx/frequency", hz.into()),
            Call::TxPath(route) => set(&mut oracle, c"/dm/sdr/0/tx/path", tx_code(route)),
            Call::Calibrate(flags) => set(&mut oracle, c"/dm/sdr/0/calibrate", flags),
            Call::TxPga(value) => set(
                &mut oracle,
                c"/dm/sdr/0/tx/gain/vga1",
                u64::from_ne_bytes(i64::from(value).to_ne_bytes()),
            ),
            Call::CreateTx => oracle.create_tx_stream(1, 64).map(|()| false),
            Call::DcLevel => {
                // `*ov = out`: the int sign-extended; its low half is the int.
                let level = oracle.get_uint(c"/dm/sdr/0/rx/dccorr").map(|word| {
                    let [a, b, c, d, ..] = word.to_le_bytes();
                    i32::from_le_bytes([a, b, c, d])
                });
                outcomes.push(level.map_or_else(Outcome::Errno, Outcome::Level));
                continue;
            }
        };
        match result {
            Ok(true) => {
                trapped = true;
                break;
            }
            Ok(false) => outcomes.push(if let Call::Calibrate(_) = call {
                let data = oracle.calibration_results().expect("readable");
                Outcome::Lo([data[0], data[1], data[2], data[3]])
            } else {
                Outcome::Errno(0)
            }),
            Err(errno) => outcomes.push(Outcome::Errno(errno)),
        }
    }
    let used = oracle.trace_len();
    let board = oracle.close();
    (outcomes, trapped, board.trace()[opened..used].to_vec())
}

/// xsdr's outcomes, as libusdr's, and its operations after the open.
fn rsl(board: SimBoard, calls: &[Call]) -> (Vec<Outcome>, Vec<Op>) {
    let sim = SimBus::new(board);
    let mut device = Device::open(sim.clone()).expect("xsdr opens the XSDR");
    let opened = sim.board().trace().len();
    let done = |result: Result<(), Error>| {
        result.map_or_else(
            |error| Outcome::Errno(errno(&error)),
            |()| Outcome::Errno(0),
        )
    };
    let outcomes = calls
        .iter()
        .map(|&call| match call {
            Call::PowerUp => done(device.power_up()),
            Call::Rate(rate) => done(device.set_sample_rate(rate, rate)),
            Call::Rx(hz) => done(device.set_rx_frequency(hz)),
            Call::Tx(hz) => done(device.set_tx_frequency(hz)),
            Call::TxPath(route) => done(device.set_tx_path(route)),
            Call::TxPga(value) => done(device.set_gain(Gain::TxPga, value).map(|_| ())),
            Call::CreateTx => done(device.create_tx_stream(TxChannels::A, 64)),
            Call::Calibrate(flags) => {
                let (channel, wanted) = calibrations(flags);
                match device.calibrate(channel, wanted) {
                    Ok(RfCalibration { rx_lo, tx_lo, .. }) => {
                        let [a, b] = rx_lo.unwrap_or_default();
                        let [c, d] = tx_lo.unwrap_or_default();
                        Outcome::Lo([a, b, c, d])
                    }
                    Err(error) => Outcome::Errno(errno(&error)),
                }
            }
            Call::DcLevel => device
                .rx_dc_level()
                .map_or_else(|error| Outcome::Errno(errno(&error)), Outcome::Level),
        })
        .collect();
    (outcomes, sim.board().trace()[opened..].to_vec())
}

/// The errno libusdr returns where xsdr returns `error`.
fn errno(error: &Error) -> i32 {
    match error {
        Error::NoChannel | Error::InvalidPath | Error::NcoOutOfRange(_) => -EINVAL,
        Error::TuneOutOfRange(_) => -ERANGE,
        Error::WeakCalibrationSignal => -ENAVAIL,
        Error::EstimatorStalled => -EAGAIN,
        other => panic!("no libusdr counterpart: {other}"),
    }
}

/// The scenarios' opening: power, a 40 MS/s rate, the LOs and TX band 1.
fn setup() -> Vec<Call> {
    vec![
        Call::PowerUp,
        Call::Rate(40_000_000),
        Call::Rx(1_000_000_000),
        Call::Tx(1_200_000_000),
        Call::TxPath(TxRoute::Band1),
    ]
}

/// Panics, naming `scenario`, where xsdr leaves libusdr on `board`; returns libusdr's
/// outcomes. A trap in libusdr ends its comparison there.
fn assert_same(board: impl Fn() -> SimBoard, calls: &[Call], scenario: &str) -> Vec<Outcome> {
    let (expected, trapped, expected_ops) = libusdr(board(), calls);
    let (actual, actual_ops) = rsl(board(), calls);
    let compared = if trapped {
        expected.len()
    } else {
        actual.len()
    };
    assert_eq!(&actual[..compared], &expected[..], "{scenario}: outcomes");
    let end = if trapped {
        expected_ops.len()
    } else {
        expected_ops.len().max(actual_ops.len())
    };
    if let Some(at) = (0..end).find(|&i| expected_ops.get(i) != actual_ops.get(i)) {
        panic!(
            "{scenario}: xsdr diverges at op {at} of {}: libusdr {}, xsdr {}",
            expected_ops.len(),
            expected_ops.get(at).map_or("(end)".into(), Op::to_string),
            actual_ops.get(at).map_or("(end)".into(), Op::to_string),
        );
    }
    expected
}

/// A looped board on DUC/DDC gateware.
fn looped() -> SimBoard {
    SimBoard::new(Variant::Xsdr)
        .with_hwid(DUC_DDC)
        .with_loopback(LOOP)
}

#[test]
fn each_calibration_alone_matches_libusdr_and_finds_the_loops_offsets() {
    for (flags, name) in [
        (RX_LO, "RX LO"),
        (TX_LO, "TX LO"),
        (RX_IQ, "RX I/Q"),
        (TX_IQ, "TX I/Q"),
    ] {
        let mut calls = setup();
        calls.extend([Call::Calibrate(flags), Call::DcLevel]);
        let outcomes = assert_same(looped, &calls, name);
        let found = outcomes[setup().len()];
        // Characterization: the codes libusdr's searches reached over `LOOP` at the vendored
        // PIN (xsdr matches them above). The RX LO's are within a step of the offset
        // (12.3, -7.6) they cancel; the TX LO's cancel the leakage (83.4, -51.2) as the TX
        // imbalance skews it.
        match flags {
            RX_LO => assert_eq!(found, Outcome::Lo([-12, 7, 0, 0])),
            TX_LO => assert_eq!(found, Outcome::Lo([0, 0, -82, 53])),
            _ => assert_eq!(found, Outcome::Lo([0; 4])),
        }
    }
}

#[test]
fn every_calibration_on_either_channel_matches_libusdr() {
    for flags in [ALL, ALL | CHANNEL_B] {
        let mut calls = setup();
        calls.extend([Call::DcLevel, Call::Calibrate(flags), Call::DcLevel]);
        assert_same(looped, &calls, &format!("flags {flags:#x}"));
    }
}

#[test]
fn kept_settings_and_an_external_loop_match_libusdr() {
    let external = || {
        SimBoard::new(Variant::Xsdr)
            .with_hwid(DUC_DDC)
            .with_loopback(Loopback {
                external: true,
                ..LOOP
            })
    };
    for flags in [RX_LO | KEEP, ALL | KEEP, TX_LO | TX_IQ | EXTERNAL] {
        let mut calls = setup();
        calls.push(Call::Calibrate(flags));
        assert_same(external, &calls, &format!("flags {flags:#x}"));
    }
}

#[test]
fn a_silent_receiver_skips_the_iq_searches_as_libusdr_does() {
    // No loop: the RX I/Q tone stays at -180 dB, which the TX stream-down after it hides,
    // and the TX I/Q one too, which libusdr skips.
    let mut calls = setup();
    calls.extend([Call::Calibrate(RX_IQ), Call::Calibrate(TX_IQ)]);
    let silent = || SimBoard::new(Variant::Xsdr).with_hwid(DUC_DDC);
    let outcomes = assert_same(silent, &calls, "silent");
    assert_eq!(outcomes[setup().len()..], [Outcome::Lo([0; 4]); 2]);
}

#[test]
fn a_tx_calibration_without_a_tx_band_is_refused_like_libusdr() {
    let mut calls = setup();
    calls.pop();
    calls.push(Call::Calibrate(TX_LO));
    let outcomes = assert_same(looped, &calls, "no TX band");
    assert_eq!(outcomes.last(), Some(&Outcome::Errno(-EINVAL)));
}

#[test]
fn off_duc_ddc_gateware_libusdr_traps_in_the_restore_where_xsdr_divides_by_one() {
    let board = || {
        SimBoard::new(Variant::Xsdr)
            .with_hwid(TX_MMCM)
            .with_loopback(LOOP)
    };
    let mut calls = setup();
    calls.push(Call::Calibrate(RX_LO));
    let (_, trapped, _) = libusdr(board(), &calls);
    assert!(
        trapped,
        "libusdr divides by s_tx_int, 0 off DUC/DDC gateware"
    );
    assert_same(board, &calls, "TX MMCM only");
    // Kept settings skip the restore, so the runs match whole.
    calls.pop();
    calls.push(Call::Calibrate(RX_LO | TX_LO | KEEP));
    let (_, trapped, _) = libusdr(board(), &calls);
    assert!(!trapped);
    assert_same(board, &calls, "TX MMCM only, kept");
}

#[test]
fn a_running_tx_stream_a_missing_lo_and_a_negative_tx_gain_match_libusdr() {
    let streaming = || looped().with_gateware_build(TX_GATEWARE);
    let mut calls = setup();
    calls.extend([Call::CreateTx, Call::Calibrate(ALL)]);
    let outcomes = assert_same(streaming, &calls, "TX streaming");
    assert!(
        outcomes[..calls.len() - 1]
            .iter()
            .all(|o| *o == Outcome::Errno(0)),
        "TX streaming: setup ran: {outcomes:?}"
    );

    // No RX LO: the TX calibrations set the RX filter to 1 MHz and disable SXR after.
    let mut calls = setup();
    calls.retain(|call| !matches!(call, Call::Rx(_)));
    calls.push(Call::Calibrate(TX_LO | TX_IQ));
    assert_same(looped, &calls, "no RX LO");

    // No TX LO: the RX I/Q calibration tunes SXT for its tone, then disables it.
    let mut calls = setup();
    calls.retain(|call| !matches!(call, Call::Tx(_)));
    calls.push(Call::Calibrate(RX_IQ));
    assert_same(looped, &calls, "no TX LO");

    // A negative TX gain correction, which libusdr's restore always rewrites.
    let mut calls = setup();
    calls.extend([Call::TxPga(-5), Call::Calibrate(RX_LO)]);
    let outcomes = assert_same(looped, &calls, "negative TX PGA");
    assert!(
        outcomes[..calls.len() - 1]
            .iter()
            .all(|o| *o == Outcome::Errno(0)),
        "negative TX PGA: setup ran: {outcomes:?}"
    );
}
