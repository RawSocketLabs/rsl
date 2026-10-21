//! xsdr's `fe=` hints, front-end controls and oscillator trim against libusdr's, operation
//! for operation: the open's probe with each hint, then each control's operations and
//! whether it succeeded.
//!
//! The XSDR's libusdr fails its open when a hint finds nothing (`-ENODEV`, which the uSDR
//! ignores), so does xsdr; neither powers anything down after. The XTRX has no connector
//! and ignores hints.
//!
//! No divergence is whitelisted. Not compared: the XTRX's oscillator trim. libusdr's
//! `dev_m2_lm7_1_dev_dac_vctcxo_set` looks the front end up first, through a pointer the
//! XTRX's open never sets (`malloc`ed, uninitialised), which crashes the oracle; xsdr
//! trims the DAC as `xsdr_trim_dac_vctcxo` would (`tests/open.rs`).

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use std::ffi::CStr;

use sim_bus::SimBus;
use xsdr::{
    Device, Error, Exm2peOptions, FrontEndKind, FrontEndSelection, PciefeAttenuation,
    PciefeDuplexer, PciefeOptions, PciefeRxFilter, PciefeSetting, PciefeTxFilter,
    XmassCalibrationPath,
};
use xsdr_oracle::Oracle;
use xsdr_sim::{FrontEnd, Op, SimBoard, Variant};

/// `ENODEV`: a hint that finds nothing.
const ENODEV: i32 = 19;

/// Panics at the first operation where `actual` leaves `expected`.
fn assert_same_ops(expected: &[Op], actual: &[Op], scenario: &str) {
    if let Some(at) =
        (0..expected.len().max(actual.len())).find(|&i| expected.get(i) != actual.get(i))
    {
        panic!(
            "{scenario}: xsdr diverges from libusdr at op {at}: libusdr {}, xsdr {}",
            expected.get(at).map_or("(end)".into(), Op::to_string),
            actual.get(at).map_or("(end)".into(), Op::to_string),
        );
    }
}

/// An XSDR with nothing on its connector.
fn bare() -> SimBoard {
    SimBoard::new(Variant::Xsdr)
}

/// An XSDR with `front_end` attached.
fn with(front_end: FrontEnd) -> impl Fn() -> SimBoard {
    move || SimBoard::new(Variant::Xsdr).with_front_end(front_end)
}

/// Opens fresh copies of `board` through libusdr with `parameters` and through xsdr with
/// `selection`, closes whichever opened, and compares the whole traces. Returns xsdr's
/// open result with the front end it reports, and libusdr's errno.
fn assert_open_matches(
    board: impl Fn() -> SimBoard,
    parameters: &CStr,
    selection: FrontEndSelection,
) -> (Result<Option<FrontEndKind>, Error>, i32) {
    let (reference, errno) = match Oracle::open_with(board(), parameters) {
        Ok(oracle) => (oracle.close(), 0),
        Err(error) => (*error.board, error.errno),
    };

    let sim = SimBus::new(board());
    let result = Device::open_with(sim.clone(), &selection).map(|device| {
        let kind = device.front_end();
        device.close().expect("xsdr closes the board");
        kind
    });
    assert_same_ops(
        reference.trace(),
        sim.board().trace(),
        &format!("{parameters:?}"),
    );
    assert_eq!(result.is_ok(), errno == 0, "{parameters:?}: {result:?}");
    (result, errno)
}

/// libusdr's call for one step.
type LibusdrCall = Box<dyn Fn(&mut Oracle) -> Result<(), i32>>;
/// xsdr's call for one step.
type XsdrCall = Box<dyn Fn(&mut Device) -> Result<(), Error>>;

/// One control applied to both drivers.
struct Step {
    /// What the step does, for failure messages.
    name: String,
    /// The libusdr call.
    libusdr: LibusdrCall,
    /// The xsdr call.
    xsdr: XsdrCall,
}

/// A step applying one `PCIe` front-end command: the `FECMD_*` address and data, and the
/// typed setting.
fn pciefe_step(command: u8, value: u16, setting: PciefeSetting) -> Step {
    Step {
        name: format!("{setting:?}"),
        libusdr: Box::new(move |oracle| oracle.pciefe_command(command, value)),
        xsdr: Box::new(move |device| device.configure_pciefe(setting)),
    }
}

/// A step setting the oscillator trim DAC.
fn dac_step(code: u16) -> Step {
    Step {
        name: format!("oscillator DAC {code:#06x}"),
        libusdr: Box::new(move |oracle| oracle.set_oscillator_dac(code)),
        xsdr: Box::new(move |device| device.set_oscillator_dac(code)),
    }
}

/// A step retuning the synchronization board's calibration LO; `sdr_path` uses XMASS's
/// second path.
fn calibration_step(hz: u32, sdr_path: bool) -> Step {
    Step {
        name: format!("calibration LO {hz} Hz"),
        libusdr: Box::new(move |oracle| oracle.set_sync_calibration_frequency(hz.into(), sdr_path)),
        xsdr: Box::new(move |device| device.set_sync_calibration_frequency(hz)),
    }
}

/// A step routing XMASS's calibration signal, with libusdr's number for the path.
fn calibration_path_step(code: u64, path: XmassCalibrationPath) -> Step {
    Step {
        name: format!("calibration path {path:?}"),
        libusdr: Box::new(move |oracle| oracle.set_sync_calibration_path(code)),
        xsdr: Box::new(move |device| device.set_sync_calibration_path(path)),
    }
}

/// Opens fresh copies of `board` in both drivers, runs `steps` on each, and compares each
/// step's operations and whether it succeeded. Returns xsdr's step results and its board
/// after the close.
fn assert_steps_match(
    board: impl Fn() -> SimBoard,
    parameters: &CStr,
    selection: FrontEndSelection,
    steps: &[Step],
) -> (Vec<Result<(), Error>>, SimBus) {
    let mut oracle = Oracle::open_with(board(), parameters).expect("libusdr opens");
    let mut expected = Vec::new();
    for step in steps {
        let start = oracle.trace_len();
        let ok = (step.libusdr)(&mut oracle).is_ok();
        expected.push((start..oracle.trace_len(), ok));
    }
    let reference = oracle.close();

    let sim = SimBus::new(board());
    let mut device = Device::open_with(sim.clone(), &selection).expect("xsdr opens");
    let mut results = Vec::new();
    for (step, (window, ok)) in steps.iter().zip(expected) {
        let start = sim.board().trace().len();
        let result = (step.xsdr)(&mut device);
        let actual: Vec<Op> = sim.board().trace()[start..].to_vec();
        assert_same_ops(&reference.trace()[window], &actual, &step.name);
        assert_eq!(result.is_ok(), ok, "{}: {result:?}", step.name);
        results.push(result);
    }
    device.close().expect("xsdr closes");
    (results, sim)
}

#[test]
fn hints_that_find_nothing_fail_the_open_like_libusdr() {
    for (parameters, selection) in [
        (c"fe=none", FrontEndSelection::None),
        (c"fe=xmass", FrontEndSelection::Xmass),
        (c"fe=supersync", FrontEndSelection::SuperSync),
        (c"fe=simplesync", FrontEndSelection::SimpleSync),
        // The connector does not admit the LSDr's front end: nothing is probed.
        (c"fe=fe1005000", FrontEndSelection::Fe1005000),
    ] {
        let (result, errno) = assert_open_matches(bare, parameters, selection);
        assert_eq!(errno, -ENODEV, "{parameters:?}");
        assert!(matches!(result, Err(Error::NoFrontEnd)), "{result:?}");
    }
}

#[test]
fn a_missing_pciefe_fails_the_open_at_its_dac_like_libusdr() {
    let (result, errno) = assert_open_matches(
        bare,
        c"fe=pciefe:v1",
        FrontEndSelection::Pciefe(PciefeOptions::default()),
    );
    assert_ne!(errno, 0);
    assert!(matches!(result, Err(Error::FrontEnd(_))), "{result:?}");
}

#[test]
fn hinted_boards_come_up_like_libusdr() {
    let mut options = Exm2peOptions::default();
    options.oscillator = Some(true);
    options.gps = Some(false);
    for (board, parameters, selection, kind) in [
        (
            FrontEnd::Pciefe,
            c"fe=pciefe:v1",
            FrontEndSelection::Pciefe(PciefeOptions::default()),
            FrontEndKind::Pciefe,
        ),
        (
            FrontEnd::Exm2pe,
            c"fe=exm2pe:osc_on:gps_off",
            FrontEndSelection::Exm2pe(options),
            FrontEndKind::Exm2pe { dac: true },
        ),
        (
            FrontEnd::Xmass,
            c"fe=xmass",
            FrontEndSelection::Xmass,
            FrontEndKind::Xmass,
        ),
        (
            FrontEnd::SuperSync,
            c"fe=supersync",
            FrontEndSelection::SuperSync,
            FrontEndKind::SuperSync,
        ),
        (
            FrontEnd::SimpleSync,
            c"fe=simplesync",
            FrontEndSelection::SimpleSync,
            FrontEndKind::SimpleSync,
        ),
    ] {
        let (result, _) = assert_open_matches(with(board), parameters, selection);
        assert_eq!(result.expect("both open"), Some(kind), "{parameters:?}");
    }
}

#[test]
fn an_xtrx_ignores_hints_like_libusdr() {
    let (result, _) = assert_open_matches(
        || SimBoard::new(Variant::Xtrx),
        c"fe=xmass",
        FrontEndSelection::Xmass,
    );
    assert_eq!(result.expect("both open"), None);
}

#[test]
fn pciefe_controls_match_libusdr() {
    let steps = [
        pciefe_step(0x0, 0x1234, PciefeSetting::Dac(0x1234)),
        pciefe_step(0x1, 1, PciefeSetting::TxFilter(PciefeTxFilter::Lpf1200)),
        pciefe_step(
            0x2,
            2,
            PciefeSetting::RxFilter(PciefeRxFilter::Bpf2100To3000),
        ),
        pciefe_step(0x3, 4, PciefeSetting::Duplexer(PciefeDuplexer::Band7)),
        pciefe_step(0x4, 1, PciefeSetting::Loopback(true)),
        pciefe_step(0x5, 0b1101, PciefeSetting::Leds([true, false, true, true])),
        pciefe_step(0x6, 3, PciefeSetting::Attenuation(PciefeAttenuation::Db18)),
        pciefe_step(0x7, 1, PciefeSetting::Lna(true)),
        pciefe_step(0x8, 1, PciefeSetting::Pa(true)),
        pciefe_step(0x9, 1, PciefeSetting::Gps(true)),
        pciefe_step(0xa, 1, PciefeSetting::Oscillator(true)),
        dac_step(0x4321),
    ];
    let (results, sim) =
        assert_steps_match(with(FrontEnd::Pciefe), c"", FrontEndSelection::Auto, &steps);
    assert!(results.iter().all(Result::is_ok), "{results:?}");
    assert_eq!(sim.board().front_end_dac_code(), Some(0x4321));
}

#[test]
fn the_breakout_oscillator_dac_matches_libusdr() {
    let mut options = Exm2peOptions::default();
    options.gps = Some(true);
    let selection = FrontEndSelection::Exm2pe(options);
    let (results, sim) = assert_steps_match(
        with(FrontEnd::Exm2pe),
        c"fe=exm2pe:gps_on",
        selection,
        &[dac_step(0xbeef)],
    );
    assert!(results[0].is_ok(), "{results:?}");
    assert_eq!(sim.board().front_end_dac_code(), Some(0xbeef));
}

#[test]
fn boards_without_an_oscillator_dac_refuse_it_untouched_like_libusdr() {
    // The empty connector's breakout without a DAC (libusdr's `-EACCES`), and the clock
    // boards, where libusdr falls through to the XSDR's trim, which it has not
    // (`-ENOTSUP`).
    let (results, _) = assert_steps_match(bare, c"", FrontEndSelection::Auto, &[dac_step(1)]);
    assert!(
        matches!(results[..], [Err(Error::FrontEnd(_))]),
        "{results:?}"
    );
    for (board, parameters, selection) in [
        (
            FrontEnd::SuperSync,
            c"fe=supersync",
            FrontEndSelection::SuperSync,
        ),
        (FrontEnd::Xmass, c"fe=xmass", FrontEndSelection::Xmass),
    ] {
        let (results, _) = assert_steps_match(with(board), parameters, selection, &[dac_step(1)]);
        assert!(
            matches!(results[..], [Err(Error::NoOscillatorDac)]),
            "{board:?}: {results:?}"
        );
    }
}

#[test]
fn pciefe_commands_need_a_pciefe() {
    let step = pciefe_step(0x7, 1, PciefeSetting::Lna(true));
    let (results, _) = assert_steps_match(bare, c"", FrontEndSelection::Auto, &[step]);
    assert!(
        matches!(results[..], [Err(Error::FrontEndMissing("pciefe"))]),
        "{results:?}"
    );
}

/// LO frequencies the controls try: off, at and around the cut-offs, typical, the edges of
/// what the LMK05318 reaches, and beyond.
const CALIBRATION_HZ: [u32; 14] = [
    444_000_000,
    100_000_000,
    0,
    3_099_999,
    3_100_000,
    3_500_000,
    10_000_000,
    25_000_001,
    402_529_374,
    1_000_000_000,
    1_250_000_000,
    3_000_000_000,
    13_020_833,
    250_000_000,
];

#[test]
fn xmass_calibration_controls_match_libusdr() {
    let mut steps: Vec<Step> = CALIBRATION_HZ
        .iter()
        .enumerate()
        .map(|(index, &hz)| calibration_step(hz, index % 2 == 1))
        .collect();
    steps.extend([
        calibration_path_step(1, XmassCalibrationPath::Lo),
        calibration_path_step(2, XmassCalibrationPath::Noise),
        calibration_path_step(3, XmassCalibrationPath::LoToLna3),
        calibration_path_step(4, XmassCalibrationPath::NoiseToLna3),
        calibration_path_step(0, XmassCalibrationPath::Off),
    ]);
    let (results, _) =
        assert_steps_match(with(FrontEnd::Xmass), c"", FrontEndSelection::Auto, &steps);
    assert!(
        results.iter().any(Result::is_err) && results.iter().any(Result::is_ok),
        "both reachable and unreachable LOs: {results:?}"
    );

    for &hz in &CALIBRATION_HZ {
        let mut oracle = Oracle::open(with(FrontEnd::Xmass)()).expect("open");
        let _ = oracle.set_sync_calibration_frequency(hz.into(), false);
        let expected = oracle
            .get_uint(c"/dm/sync/cal/freq")
            .expect("XMASS reads it");
        let _ = oracle.close();
        let mut device = Device::open(SimBus::new(with(FrontEnd::Xmass)())).expect("opens");
        let _ = device.set_sync_calibration_frequency(hz);
        let actual = device.sync_calibration_frequency().expect("XMASS reads it");
        assert_eq!(u64::from(actual), expected, "after {hz} Hz");
    }
}

#[test]
fn simplesync_lo_controls_match_libusdr() {
    let steps: Vec<Step> = CALIBRATION_HZ
        .iter()
        .map(|&hz| calibration_step(hz, false))
        .collect();
    assert_steps_match(
        with(FrontEnd::SimpleSync),
        c"",
        FrontEndSelection::Auto,
        &steps,
    );
}

#[test]
fn calibration_controls_need_a_synchronization_board() {
    let sim = SimBus::new(bare());
    let mut device = Device::open(sim.clone()).expect("opens");
    let start = sim.board().trace().len();
    assert!(matches!(
        device.set_sync_calibration_frequency(10_000_000),
        Err(Error::FrontEndMissing(_))
    ));
    assert!(matches!(
        device.set_sync_calibration_path(XmassCalibrationPath::Lo),
        Err(Error::FrontEndMissing("xmass"))
    ));
    assert!(matches!(
        device.sync_calibration_frequency(),
        Err(Error::FrontEndMissing("xmass"))
    ));
    assert_eq!(sim.board().trace().len(), start, "nothing driven");

    let mut oracle = Oracle::open(bare()).expect("open");
    let start = oracle.trace_len();
    assert!(
        oracle
            .set_sync_calibration_frequency(10_000_000, false)
            .is_err()
    );
    assert!(oracle.set_sync_calibration_path(1).is_err());
    assert_eq!(oracle.trace_len(), start, "libusdr has no such paths");
    let _ = oracle.close();
}
