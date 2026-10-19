//! xsdr's RF and baseband controls against libusdr's device-manager paths, operation
//! for operation, on fresh simulated XSDRs: explicit RX/TX paths and the RF loopback
//! (`/dm/sdr/0/{rx,tx}/path`, `.../rfic_path`), TDD tuning, the baseband NCOs
//! (`.../frequency/bb`, `/dm/sdr/0/tfe/nco/frequency`), the DC and I/Q corrections
//! (`.../dccorr`, `.../dccorrmode`, `.../phgaincorr`), the TX generator and mute
//! (`/dm/sdr/0/tfe/*`), the baseband loopback (`/dm/sdr/0/bb/loopback`) and the
//! `LimeLight` receive source (`/debug/hw/lms7002m/0/rxlml`). Everything between the open
//! and the close is compared.
//!
//! The boards report gateware with a TX MMCM, as in `rate_parity.rs`; a sample rate runs
//! first where a scenario needs RX running or the converter clock.
//!
//! Whitelisted divergences: correction codes libusdr masks into its registers are refused
//! by xsdr before any bus access (`out_of_range_corrections_are_masked_by_libusdr_and_refused_by_xsdr`);
//! an NCO before any sample rate, where libusdr divides by zero, is refused by xsdr
//! and not run in libusdr (`tests/controls.rs`).

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use core::ffi::CStr;

use sim_bus::SimBus;
use xsdr::{
    Device, Direction, Error, Gain, IqCorrection, LmlRxMode, Mac, RfPath, RxRoute, TxGenerator,
    TxRoute,
};
use xsdr_oracle::Oracle;
use xsdr_sim::{Op, SimBoard, Variant};

/// `EINVAL`.
const EINVAL: i32 = 22;
/// `ERANGE`.
const ERANGE: i32 = 34;
/// An XSDR with a TX MMCM.
const TX_MMCM: u32 = 0x0030_0020;

/// One control, as libusdr's path and value and xsdr's method.
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
    /// A gain.
    Gain(Gain, i32),
    /// `/dm/sdr/0/rx/path`.
    RxPath(RxRoute),
    /// `/dm/sdr/0/tx/path`.
    TxPath(TxRoute),
    /// `/dm/sdr/0/rx/rfic_path`.
    RfPath(RfPath),
    /// `/dm/sdr/0/{rx,tx}/frequency/bb`.
    Nco(Direction, i32),
    /// `/dm/sdr/0/tdd/frequency`.
    Tdd(u32),
    /// `/dm/sdr/0/tfe/nco/frequency`.
    TxNcoWord(Mac, i32),
    /// `/dm/sdr/0/{rx,tx}/dccorr`.
    DcOffset(Direction, Mac, i16, i16),
    /// `/dm/sdr/0/rx/dccorrmode`.
    DcCorrectorOff(Mac),
    /// `/dm/sdr/0/{rx,tx}/phgaincorr`.
    Iq(Direction, Mac, IqCorrection),
    /// `/dm/sdr/0/tfe/generator/{enable,const}`.
    Generator(Mac, TxGenerator),
    /// `/dm/sdr/0/tfe/generator/tone` with its two low bits; xsdr's
    /// [`TxGenerator::Dc`] with I `value & 2` and Q `value & 1`, the levels libusdr passes.
    Tone(Mac, u8),
    /// `/dm/sdr/0/bb/loopback`.
    BasebandLoopback,
    /// `/dm/sdr/0/tfe/antcfg`.
    MuteTx(Mac),
    /// `/debug/hw/lms7002m/0/rxlml`.
    LmlRx(LmlRxMode),
}

/// libusdr's `rfic_lms7_rf_path_t` code for an RX path.
const fn rx_code(route: RxRoute) -> u64 {
    match route {
        RxRoute::LnaL => 0,
        RxRoute::LnaH => 1,
        RxRoute::LnaW => 2,
        RxRoute::LoopbackL => 3,
        RxRoute::LoopbackW => 4,
        RxRoute::LoopbackH => 5,
        RxRoute::Auto => 10,
    }
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

/// libusdr's `LMS7_CH_*` code.
fn mac_code(mac: Mac) -> u64 {
    match mac {
        Mac::None => 0,
        Mac::A => 1,
        Mac::B => 2,
        Mac::Both => 3,
    }
}

/// A 16-bit two's-complement field.
fn bits16(value: i16) -> u64 {
    u64::from(u16::from_ne_bytes(value.to_ne_bytes()))
}

/// A 32-bit two's-complement field.
fn bits32(value: i32) -> u64 {
    u64::from(u32::from_ne_bytes(value.to_ne_bytes()))
}

/// libusdr's device-manager path for a gain.
fn gain_path(gain: Gain) -> &'static CStr {
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

/// The path and value libusdr's handler takes for `call` (`None` for the calls with
/// their own entry points).
fn path_value(call: Call) -> Option<(&'static CStr, u64)> {
    let direction = |direction, rx: &'static CStr, tx: &'static CStr| match direction {
        Direction::Rx => rx,
        Direction::Tx => tx,
    };
    Some(match call {
        Call::PowerUp => return None,
        Call::Rate(rate) => (c"/dm/rate/master", rate.into()),
        Call::Rx(hz) => (c"/dm/sdr/0/rx/frequency", hz.into()),
        Call::Tx(hz) => (c"/dm/sdr/0/tx/frequency", hz.into()),
        // The handlers take the u64 as an `int`: two's complement.
        Call::Gain(gain, value) => (
            gain_path(gain),
            u64::from_ne_bytes(i64::from(value).to_ne_bytes()),
        ),
        Call::RxPath(route) => (c"/dm/sdr/0/rx/path", rx_code(route)),
        Call::TxPath(route) => (c"/dm/sdr/0/tx/path", tx_code(route)),
        Call::RfPath(RfPath::Rx(route)) => (c"/dm/sdr/0/rx/rfic_path", rx_code(route)),
        Call::RfPath(RfPath::Tx(route)) => (c"/dm/sdr/0/rx/rfic_path", tx_code(route)),
        Call::Nco(dir, hz) => (
            direction(
                dir,
                c"/dm/sdr/0/rx/frequency/bb",
                c"/dm/sdr/0/tx/frequency/bb",
            ),
            bits32(hz),
        ),
        Call::Tdd(hz) => (c"/dm/sdr/0/tdd/frequency", hz.into()),
        Call::TxNcoWord(mac, word) => (
            c"/dm/sdr/0/tfe/nco/frequency",
            mac_code(mac) << 32 | bits32(word),
        ),
        Call::DcOffset(dir, mac, i, q) => (
            direction(dir, c"/dm/sdr/0/rx/dccorr", c"/dm/sdr/0/tx/dccorr"),
            mac_code(mac) << 32 | bits16(i) << 16 | bits16(q),
        ),
        Call::DcCorrectorOff(mac) => (c"/dm/sdr/0/rx/dccorrmode", mac_code(mac) << 32),
        Call::Iq(dir, mac, iq) => (
            direction(dir, c"/dm/sdr/0/rx/phgaincorr", c"/dm/sdr/0/tx/phgaincorr"),
            bits16(iq.phase) << 48
                | mac_code(mac) << 32
                | u64::from(iq.gain_q) << 16
                | u64::from(iq.gain_i),
        ),
        Call::Generator(mac, TxGenerator::Off) => {
            (c"/dm/sdr/0/tfe/generator/enable", mac_code(mac) << 32)
        }
        Call::Generator(mac, TxGenerator::Dc { i, q }) => (
            c"/dm/sdr/0/tfe/generator/const",
            mac_code(mac) << 32 | bits16(i) << 16 | bits16(q),
        ),
        Call::Tone(mac, bits) => (
            c"/dm/sdr/0/tfe/generator/tone",
            mac_code(mac) << 32 | u64::from(bits),
        ),
        Call::BasebandLoopback => (c"/dm/sdr/0/bb/loopback", 1),
        Call::MuteTx(mac) => (
            c"/dm/sdr/0/tfe/antcfg",
            match mac {
                Mac::None => 0,
                Mac::A => 2,
                Mac::B => 1,
                Mac::Both => 3,
            },
        ),
        Call::LmlRx(mode) => (
            c"/debug/hw/lms7002m/0/rxlml",
            match mode {
                LmlRxMode::Normal => 0,
                LmlRxMode::DigitalLoopback => 1,
                LmlRxMode::Lfsr => 2,
            },
        ),
    })
}

/// libusdr's results and the operations between its open and close.
fn libusdr(board: SimBoard, calls: &[Call]) -> (Vec<i32>, Vec<Op>) {
    let mut oracle = Oracle::open(board).expect("libusdr opens the board");
    let opened = oracle.trace_len();
    let results = calls
        .iter()
        .map(|&call| {
            let result = match path_value(call) {
                None => oracle.power_up(),
                // SAFETY: every handler these paths reach takes its value as a number (the
                // path handlers read a pointer only above 4096, which no path code reaches).
                Some((path, value)) => unsafe { oracle.set_uint(path, value) },
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
            errno(match call {
                Call::PowerUp => device.power_up(),
                Call::Rate(rate) => device.set_sample_rate(rate, rate),
                Call::Rx(hz) => device.set_rx_frequency(hz),
                Call::Tx(hz) => device.set_tx_frequency(hz),
                Call::Gain(gain, value) => device.set_gain(gain, value).map(|_| ()),
                Call::RxPath(route) => device.set_rx_path(route),
                Call::TxPath(route) => device.set_tx_path(route),
                Call::RfPath(path) => device.set_rf_path(path),
                Call::Nco(direction, hz) => device.set_baseband_frequency(direction, hz),
                Call::Tdd(hz) => device.set_tdd_frequency(hz),
                Call::TxNcoWord(mac, word) => device.set_tx_nco_word(mac, word),
                Call::DcOffset(direction, mac, i, q) => device.set_dc_offset(direction, mac, i, q),
                Call::DcCorrectorOff(mac) => device.disable_rx_dc_corrector(mac),
                Call::Iq(direction, mac, iq) => device.set_iq_correction(direction, mac, iq),
                Call::Generator(mac, generator) => device.set_tx_generator(mac, generator),
                Call::Tone(mac, bits) => device.set_tx_generator(
                    mac,
                    TxGenerator::Dc {
                        i: i16::from(bits & 2),
                        q: i16::from(bits & 1),
                    },
                ),
                Call::BasebandLoopback => device.loop_baseband(),
                Call::MuteTx(mac) => device.mute_tx(mac),
                Call::LmlRx(mode) => device.set_lml_rx_mode(mode),
            })
        })
        .collect();
    (results, sim.board().trace()[opened..].to_vec())
}

/// The errno libusdr returns where xsdr returns `result`.
fn errno(result: Result<(), Error>) -> i32 {
    match result {
        Ok(()) => 0,
        Err(Error::NoChannel | Error::InvalidPath | Error::NcoOutOfRange(_)) => -EINVAL,
        Err(Error::TuneOutOfRange(_)) => -ERANGE,
        Err(other) => panic!("no libusdr counterpart: {other}"),
    }
}

/// Panics, naming `scenario`, where xsdr leaves libusdr; returns libusdr's results.
fn assert_same(calls: &[Call], scenario: &str) -> Vec<i32> {
    let board = || SimBoard::new(Variant::Xsdr).with_hwid(TX_MMCM);
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
fn every_receive_path_and_loopback_matches_libusdr_with_rx_running() {
    let mut calls = vec![
        Call::PowerUp,
        Call::Rx(1_000_000_000),
        Call::Tx(1_500_000_000),
        Call::Rate(10_000_000),
        // A pad loss for the loopback's restore: libusdr stores -20 dB's reported gain as
        // 236 in its byte.
        Call::Gain(Gain::TxPad, -20),
    ];
    for route in [
        RxRoute::LnaL,
        RxRoute::LnaH,
        RxRoute::LnaW,
        RxRoute::LoopbackL,
        RxRoute::LoopbackW,
        RxRoute::LoopbackH,
        RxRoute::Auto,
        RxRoute::LoopbackW,
        RxRoute::LnaH,
    ] {
        calls.push(Call::RxPath(route));
        // A new LO: the automatic band follows; an explicit path only reselects the banks.
        calls.push(Call::Rx(2_500_000_000));
        calls.push(Call::Rx(900_000_000));
    }
    calls.push(Call::Gain(Gain::TxPad, -3));
    calls.extend([
        Call::RxPath(RxRoute::LoopbackL),
        Call::RxPath(RxRoute::LnaL),
        Call::RxPath(RxRoute::Auto),
    ]);
    let results = assert_same(&calls, "RX paths");
    assert!(results.iter().all(|&r| r == 0), "every path is accepted");
}

#[test]
fn every_transmit_path_matches_libusdr_in_and_out_of_loopback() {
    let mut calls = vec![Call::PowerUp, Call::Tx(3_000_000_000)];
    for route in [
        TxRoute::Band1,
        TxRoute::Band2,
        TxRoute::W,
        TxRoute::H,
        TxRoute::Auto,
    ] {
        calls.push(Call::TxPath(route));
    }
    calls.push(Call::RxPath(RxRoute::LoopbackH));
    for route in [TxRoute::Band2, TxRoute::H, TxRoute::Auto, TxRoute::Band1] {
        calls.push(Call::TxPath(route));
    }
    // The automatic TX path ended the loopback without restoring anything; the RX path
    // turning it off again finds it off.
    calls.push(Call::RxPath(RxRoute::LnaW));
    let results = assert_same(&calls, "TX paths");
    assert!(results.iter().all(|&r| r == 0));
}

#[test]
fn rf_paths_select_both_banks_first_and_refuse_w_and_h_like_libusdr() {
    let calls = [
        Call::PowerUp,
        Call::TxNcoWord(Mac::A, 0),
        Call::RfPath(RfPath::Rx(RxRoute::LnaL)),
        Call::TxNcoWord(Mac::B, 0),
        Call::RfPath(RfPath::Tx(TxRoute::Band2)),
        Call::RfPath(RfPath::Rx(RxRoute::LoopbackW)),
        Call::RfPath(RfPath::Tx(TxRoute::Auto)),
        Call::TxNcoWord(Mac::A, 0),
        Call::RfPath(RfPath::Tx(TxRoute::W)),
        Call::RfPath(RfPath::Tx(TxRoute::H)),
        Call::RfPath(RfPath::Rx(RxRoute::Auto)),
    ];
    let results = assert_same(&calls, "RF paths");
    assert_eq!(
        results[8..10],
        [-EINVAL, -EINVAL],
        "W and H reach the RX selection"
    );
}

#[test]
fn paths_before_a_power_up_are_refused_like_libusdr() {
    let calls = [
        Call::RxPath(RxRoute::LnaH),
        Call::RxPath(RxRoute::LoopbackL),
        Call::TxPath(TxRoute::Band2),
        Call::RfPath(RfPath::Tx(TxRoute::Band1)),
    ];
    let results = assert_same(&calls, "no power-up");
    assert_eq!(
        results,
        [-EINVAL, -EINVAL, -EINVAL, 0],
        "no bank is selected, until the RF path selects both"
    );
}

#[test]
fn baseband_ncos_match_libusdr_running_or_not() {
    let calls = [
        Call::PowerUp,
        Call::Rate(1_000_000),
        Call::Nco(Direction::Tx, 100_000),
        Call::Nco(Direction::Rx, -250_000),
        Call::Nco(Direction::Rx, 0),
        Call::Nco(Direction::Rx, 13_333_333),
        // 1 MS/s runs CGEN at 128 MHz, so the converters at 32 MHz: half is 16 MHz, whose
        // word 2^31 libusdr converts to INT32_MIN.
        Call::Nco(Direction::Rx, 16_000_000),
        Call::Nco(Direction::Rx, 16_000_001),
        Call::Nco(Direction::Rx, -16_000_001),
        Call::Nco(Direction::Rx, -16_000_000),
        Call::Nco(Direction::Tx, -1_000_000),
        Call::Rate(2_000_000),
        Call::Nco(Direction::Rx, 700_000),
    ];
    let results = assert_same(&calls, "NCOs");
    assert_eq!(
        results[7..9],
        [-EINVAL, -EINVAL],
        "beyond half the ADC clock"
    );
    assert_eq!(results.iter().filter(|&&r| r != 0).count(), 2);
}

#[test]
fn leaving_the_loopback_restores_the_stored_tx_band_not_the_los_like_libusdr() {
    // TX idles on the automatic path at 3 GHz, band 2's range, while the band in use is
    // still the first (`tx_cfg_path` 0): turning the loopback off restores that one.
    let calls = [
        Call::PowerUp,
        Call::Tx(3_000_000_000),
        Call::RxPath(RxRoute::LoopbackL),
        Call::RxPath(RxRoute::LnaL),
        Call::TxPath(TxRoute::Band2),
        Call::RxPath(RxRoute::LoopbackW),
        Call::Tx(1_000_000_000),
        Call::RxPath(RxRoute::Auto),
    ];
    let results = assert_same(&calls, "loopback off");
    assert!(results.iter().all(|&r| r == 0));
}

#[test]
fn streaming_up_keeps_an_explicit_or_looped_back_band_like_libusdr() {
    // Each rate streams RX up again: on the stored band for an explicit path, in RF
    // loopback with the TX band feeding it, and following the LO when automatic.
    let calls = [
        Call::PowerUp,
        Call::Rx(3_000_000_000),
        Call::RxPath(RxRoute::LoopbackL),
        Call::Rate(5_000_000),
        Call::RxPath(RxRoute::LnaW),
        Call::Rate(10_000_000),
        Call::RxPath(RxRoute::LoopbackH),
        Call::Rx(1_000_000_000),
        Call::Rate(5_000_000),
        Call::RxPath(RxRoute::Auto),
        Call::Rate(10_000_000),
    ];
    let results = assert_same(&calls, "streaming up on a path");
    assert!(results.iter().all(|&r| r == 0));
}

#[test]
fn tdd_tuning_matches_libusdr() {
    let calls = [
        Call::PowerUp,
        Call::Tdd(2_000_000_000),
        Call::Rate(5_000_000),
        Call::Tdd(1_000_000_000),
        Call::RxPath(RxRoute::LnaL),
        Call::Tdd(2_450_000_000),
        Call::Tdd(0),
        Call::Tdd(4_100_000_000),
    ];
    let results = assert_same(&calls, "TDD");
    assert_eq!(results[7], -ERANGE, "no VCO reaches it");
}

#[test]
fn the_corrections_generator_and_loopbacks_match_libusdr() {
    let iq = |gain_i, gain_q, phase| IqCorrection {
        gain_i,
        gain_q,
        phase,
    };
    let mut calls = vec![Call::PowerUp];
    for mac in [Mac::A, Mac::B, Mac::Both] {
        calls.extend([
            Call::DcOffset(Direction::Rx, mac, 1023, -1024),
            Call::DcOffset(Direction::Tx, mac, -1, 5),
            Call::DcCorrectorOff(mac),
            Call::Iq(Direction::Rx, mac, iq(2047, 0, -2048)),
            Call::Iq(Direction::Tx, mac, iq(0, 0, 0)),
            Call::Iq(Direction::Tx, mac, iq(1, 2000, 2047)),
            Call::Iq(Direction::Rx, mac, iq(0, 0, 7)),
            Call::Generator(mac, TxGenerator::Dc { i: 0, q: 0 }),
            Call::Generator(mac, TxGenerator::Dc { i: -300, q: 1 }),
            Call::Generator(mac, TxGenerator::Off),
            Call::Tone(mac, 3),
            Call::Tone(mac, 0),
            Call::TxNcoWord(mac, -0x1234_5678),
            Call::TxNcoWord(mac, 0),
            Call::MuteTx(mac),
        ]);
    }
    calls.extend([
        Call::DcOffset(Direction::Rx, Mac::None, 0, 0),
        Call::Iq(Direction::Rx, Mac::None, iq(1, 1, 1)),
        Call::TxNcoWord(Mac::None, 1),
        Call::DcCorrectorOff(Mac::None),
        Call::MuteTx(Mac::None),
        Call::BasebandLoopback,
        Call::LmlRx(LmlRxMode::DigitalLoopback),
        Call::LmlRx(LmlRxMode::Lfsr),
        Call::LmlRx(LmlRxMode::Normal),
    ]);
    let results = assert_same(&calls, "corrections");
    let refused = results.iter().filter(|&&r| r == -EINVAL).count();
    assert_eq!(refused, 4, "no channel: DC offset, I/Q, TX NCO, corrector");
}

#[test]
fn out_of_range_corrections_are_masked_by_libusdr_and_refused_by_xsdr() {
    let iq = IqCorrection {
        gain_i: 2048,
        gain_q: 0,
        phase: 0,
    };
    for call in [
        Call::DcOffset(Direction::Rx, Mac::A, 1024, 0),
        Call::DcOffset(Direction::Tx, Mac::B, 0, -1025),
        Call::Iq(Direction::Rx, Mac::A, iq),
        Call::Iq(
            Direction::Tx,
            Mac::A,
            IqCorrection {
                phase: 2048,
                ..IqCorrection::default()
            },
        ),
    ] {
        let board = || SimBoard::new(Variant::Xsdr).with_hwid(TX_MMCM);
        let (expected, _) = libusdr(board(), &[Call::PowerUp, call]);
        assert_eq!(expected[1], 0, "{call:?}: libusdr masks and writes it");
        let sim = SimBus::new(board());
        let mut device = Device::open(sim.clone()).expect("opens");
        device.power_up().expect("powers up");
        let before = sim.board().trace().len();
        let result = match call {
            Call::DcOffset(direction, mac, i, q) => device.set_dc_offset(direction, mac, i, q),
            Call::Iq(direction, mac, iq) => device.set_iq_correction(direction, mac, iq),
            _ => unreachable!("only corrections"),
        };
        assert!(matches!(result, Err(Error::InvalidCorrection)), "{call:?}");
        assert_eq!(
            sim.board().trace().len(),
            before,
            "{call:?}: nothing driven"
        );
    }
}
