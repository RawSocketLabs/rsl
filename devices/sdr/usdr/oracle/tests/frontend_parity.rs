//! rsl-usdr's front-end probe and controls against libusdr's (`fe=`), operation for
//! operation over whole power-ups.

#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;
#[path = "common/thermal_gate.rs"]
mod thermal_gate;

use std::ffi::CStr;

use rsl_usdr::frontend::{
    Exm2peOptions, PciefeAttenuation, PciefeDuplexer, PciefeOptions, PciefePath, PciefeRevision,
    PciefeRxFilter, PciefeSetting, PciefeTxFilter,
};
use rsl_usdr::{Device, Error, FrontEndKind, FrontEndSelection};
use rsl_usdr_oracle::Oracle;
use rsl_usdr_sim::{BoardRevision, FrontEnd, Op, SimBoard};
use sim_bus::SimBus;
use thermal_gate::{assert_same_ops, without_thermal_gate};

/// The revision every scenario runs on; the probe does not depend on it.
const REVISION: BoardRevision = BoardRevision::Rev3;

/// rsl-usdr's power-down: LMS6002D reset, LED off, booster off (GPO banks 0, 8, 7).
const POWER_DOWN: [Op; 3] = [
    Op::RegWrite { addr: 0, value: 0 },
    Op::RegWrite {
        addr: 0,
        value: 8 << 24,
    },
    Op::RegWrite {
        addr: 0,
        value: 7 << 24,
    },
];

/// rsl-usdr's trace of a failed open without the power-down that ends it.
///
/// Intentional divergence, scoped to failed opens: when the front-end probe fails,
/// rsl-usdr powers the board down again; libusdr leaves it powered and returns.
fn without_failure_power_down(trace: &[Op]) -> &[Op] {
    let (kept, tail) = trace.split_at(trace.len() - POWER_DOWN.len());
    assert_eq!(
        tail, POWER_DOWN,
        "a failed open ends in exactly the power-down"
    );
    kept
}

/// Opens fresh copies of `board` through libusdr with `parameters` and through rsl-usdr
/// with `selection`, closes whichever opened, and compares the whole traces. Returns
/// rsl-usdr's open result with the front end it reports.
fn assert_open_matches(
    board: impl Fn() -> SimBoard,
    parameters: &CStr,
    selection: FrontEndSelection,
) -> Result<Option<FrontEndKind>, Error> {
    let reference = match Oracle::open_with(board(), parameters) {
        Ok(oracle) => oracle.close(),
        Err(error) => *error.board,
    };

    let sim = SimBus::new(board());
    let opened = Device::builder(sim.clone()).front_end(selection).open();
    let result = opened.map(|device| {
        let kind = device.front_end();
        device.close().expect("rsl-usdr powers down");
        kind
    });

    let actual = without_thermal_gate(sim.board().trace(), REVISION);
    let actual = if result.is_ok() {
        &actual[..]
    } else {
        without_failure_power_down(&actual)
    };
    assert_same_ops(reference.trace(), actual, &format!("{parameters:?}"));
    result
}

/// A board with nothing on its connector.
fn bare() -> SimBoard {
    SimBoard::new(REVISION)
}

/// libusdr's default probe finds nothing that identifies itself and settles on the
/// breakout, which has no ID; so does rsl-usdr.
#[test]
fn the_automatic_probe_settles_on_a_breakout_without_a_dac() {
    let kind = assert_open_matches(bare, c"", FrontEndSelection::Auto).expect("both open");
    assert_eq!(kind, Some(FrontEndKind::Exm2pe { dac: false }));
}

/// `fe=` naming no board probes nothing at all.
#[test]
fn naming_no_board_probes_nothing() {
    let kind = assert_open_matches(bare, c"fe=none", FrontEndSelection::None).expect("both open");
    assert_eq!(kind, None);
}

/// libusdr admits the 100–5000 MHz front end on `LSDr` only: naming it probes nothing, as
/// [`FrontEndSelection::None`] does, which is why rsl-usdr offers no selection for it.
#[test]
fn naming_the_lsdr_front_end_probes_nothing() {
    let kind = assert_open_matches(bare, c"fe=fe1005000", FrontEndSelection::None).expect("open");
    assert_eq!(kind, None);
}

/// The clock boards each probe their chip and, finding none, leave the uSDR without a
/// front end.
#[test]
fn absent_clock_boards_are_not_found() {
    for (parameters, selection) in [
        (c"fe=xmass", FrontEndSelection::Xmass),
        (c"fe=supersync", FrontEndSelection::SuperSync),
        (c"fe=simplesync", FrontEndSelection::SimpleSync),
    ] {
        let kind = assert_open_matches(bare, parameters, selection).expect("both open");
        assert_eq!(kind, None, "{parameters:?}");
    }
}

/// A hinted `PCIe` front end that is not there fails the open where libusdr's does: its
/// expanders read the idle bus, so the bring-up reaches the DAC, which does not identify.
#[test]
fn a_missing_pciefe_fails_the_open_at_its_dac() {
    let result = assert_open_matches(
        bare,
        c"fe=pciefe:v1",
        FrontEndSelection::Pciefe(PciefeOptions::default()),
    );
    assert!(
        matches!(
            result,
            Err(Error::FrontEnd {
                board: "pciefe",
                ..
            })
        ),
        "{result:?}"
    );
}

/// On the V0 board an unrecognised DAC also fails the open.
#[test]
fn a_missing_v0_pciefe_fails_the_open_at_its_dac() {
    let mut options = PciefeOptions::default();
    options.revision = PciefeRevision::V0;
    let result = assert_open_matches(bare, c"fe=pciefev0", FrontEndSelection::Pciefe(options));
    assert!(
        matches!(
            result,
            Err(Error::FrontEnd {
                board: "pciefe",
                ..
            })
        ),
        "{result:?}"
    );
}

/// The breakout's options apply even without its DAC.
#[test]
fn a_hinted_breakout_without_a_dac_takes_its_options() {
    let mut options = Exm2peOptions::default();
    options.oscillator = Some(true);
    options.gps = Some(false);
    options.dac = Some(0x8000);
    let kind = assert_open_matches(
        bare,
        c"fe=exm2pe:osc_on:gps_off:dac_32768",
        FrontEndSelection::Exm2pe(options),
    )
    .expect("both open");
    assert_eq!(kind, Some(FrontEndKind::Exm2pe { dac: false }));
}

/// A board with the `PCIe` front end attached.
fn with_pciefe() -> SimBoard {
    SimBoard::new(REVISION).with_front_end(FrontEnd::Pciefe)
}

/// A board with the M.2-to-`PCIe` breakout and its DAC attached.
fn with_exm2pe() -> SimBoard {
    SimBoard::new(REVISION).with_front_end(FrontEnd::Exm2pe)
}

/// libusdr's call for one step.
type LibusdrCall = Box<dyn Fn(&mut Oracle) -> Result<(), i32>>;
/// rsl-usdr's call for one step.
type RslUsdrCall = Box<dyn Fn(&mut Device) -> Result<(), Error>>;

/// One control applied to both drivers.
struct Step {
    /// What the step does, for failure messages.
    name: String,
    /// The libusdr call.
    libusdr: LibusdrCall,
    /// The rsl-usdr call.
    rsl_usdr: RslUsdrCall,
}

/// A step applying one `PCIe` front-end command: the `FECMD_*` address and data, and the
/// typed setting.
fn pciefe_step(command: u8, value: u16, setting: PciefeSetting) -> Step {
    Step {
        name: format!("{setting:?}"),
        libusdr: Box::new(move |oracle| oracle.pciefe_command(command, value)),
        rsl_usdr: Box::new(move |device| device.configure_pciefe(setting)),
    }
}

/// A step setting the oscillator DAC.
fn dac_step(code: u16) -> Step {
    Step {
        name: format!("oscillator DAC {code:#06x}"),
        libusdr: Box::new(move |oracle| oracle.set_oscillator_dac(code)),
        rsl_usdr: Box::new(move |device| device.set_oscillator_dac(code)),
    }
}

/// Opens fresh copies of `board` in both drivers, runs `steps` on each, and compares each
/// step's operations and whether it succeeded. Returns rsl-usdr's step results and its
/// board after power-down.
fn assert_steps_match(
    board: impl Fn() -> SimBoard,
    parameters: &CStr,
    selection: FrontEndSelection,
    steps: &[Step],
) -> (Vec<Result<(), Error>>, SimBoard) {
    let mut oracle = Oracle::open_with(board(), parameters).expect("libusdr opens");
    let mut expected = Vec::new();
    for step in steps {
        let start = oracle.trace_len();
        let ok = (step.libusdr)(&mut oracle).is_ok();
        expected.push((start..oracle.trace_len(), ok));
    }
    let reference = oracle.close();

    let sim = SimBus::new(board());
    let mut device = Device::builder(sim.clone())
        .front_end(selection)
        .open()
        .expect("rsl-usdr opens");
    let mut results = Vec::new();
    for (step, (window, ok)) in steps.iter().zip(expected) {
        let start = sim.board().trace().len();
        let result = (step.rsl_usdr)(&mut device);
        let actual: Vec<Op> = sim.board().trace()[start..].to_vec();
        assert_same_ops(&reference.trace()[window], &actual, &step.name);
        assert_eq!(result.is_ok(), ok, "{}: {result:?}", step.name);
        results.push(result);
    }
    device.close().expect("rsl-usdr powers down");
    let board = std::mem::replace(&mut *sim.board(), SimBoard::new(REVISION));
    (results, board)
}

/// With a `PCIe` front end attached, the automatic probe brings it up exactly as libusdr
/// does.
#[test]
fn the_automatic_probe_brings_up_a_pciefe() {
    let kind = assert_open_matches(with_pciefe, c"", FrontEndSelection::Auto).expect("both open");
    assert_eq!(kind, Some(FrontEndKind::Pciefe));
}

/// Bring-up options: a named path, then overrides, the DAC and the fast lines.
#[test]
fn pciefe_options_match_libusdr() {
    let mut options = PciefeOptions::default();
    options.path = Some(PciefePath::Band2);
    options.dac = Some(1000);
    options.loopback = Some(true);
    options.attenuation = Some(PciefeAttenuation::Db12);
    options.gps = Some(true);
    options.pa = Some(false);
    let kind = assert_open_matches(
        with_pciefe,
        c"fe=pciefe:path_band2:dac_1000:lb_on:attn_12:gps_on:pa_off",
        FrontEndSelection::Pciefe(options),
    )
    .expect("both open");
    assert_eq!(kind, Some(FrontEndKind::Pciefe));
}

/// The V0 board's expander layout, without fast lines.
#[test]
fn v0_pciefe_options_match_libusdr() {
    let mut options = PciefeOptions::default();
    options.revision = PciefeRevision::V0;
    options.path = Some(PciefePath::Trx2100To3000);
    options.oscillator = Some(true);
    options.attenuation = Some(PciefeAttenuation::Db6);
    let kind = assert_open_matches(
        with_pciefe,
        c"fe=pciefev0:path_trx2100_3000:osc_on:attn_6",
        FrontEndSelection::Pciefe(options),
    )
    .expect("both open");
    assert_eq!(kind, Some(FrontEndKind::Pciefe));
}

/// Every `PCIe` front-end command, in `pciefe_cmd.yaml`'s encoding.
fn pciefe_commands() -> Vec<Step> {
    vec![
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
        pciefe_step(0x1, 3, PciefeSetting::TxFilter(PciefeTxFilter::Bypass)),
        pciefe_step(
            0x2,
            3,
            PciefeSetting::RxFilter(PciefeRxFilter::Bpf3000To4200),
        ),
        pciefe_step(0x3, 0, PciefeSetting::Duplexer(PciefeDuplexer::Bypass)),
        pciefe_step(0x4, 0, PciefeSetting::Loopback(false)),
        pciefe_step(0x6, 1, PciefeSetting::Attenuation(PciefeAttenuation::Db6)),
    ]
}

/// Each command on the V1 board, where loopback and attenuation move fast lines only.
#[test]
fn pciefe_controls_match_libusdr() {
    let (_, board) = assert_steps_match(
        with_pciefe,
        c"",
        FrontEndSelection::Auto,
        &pciefe_commands(),
    );
    assert_eq!(board.front_end_dac_code(), Some(0x1234));
}

/// Each command on the V0 board, where every command rewrites the expanders.
#[test]
fn v0_pciefe_controls_match_libusdr() {
    let mut options = PciefeOptions::default();
    options.revision = PciefeRevision::V0;
    assert_steps_match(
        with_pciefe,
        c"fe=pciefev0",
        FrontEndSelection::Pciefe(options),
        &pciefe_commands(),
    );
}

/// The oscillator DAC on the `PCIe` front end.
#[test]
fn the_pciefe_oscillator_dac_matches_libusdr() {
    let (_, board) = assert_steps_match(
        with_pciefe,
        c"",
        FrontEndSelection::Auto,
        &[dac_step(0x4321), dac_step(0)],
    );
    assert_eq!(board.front_end_dac_code(), Some(0));
}

/// The breakout with its DAC, named so the probe does not stop at the `PCIe` front end, and
/// its oscillator DAC.
#[test]
fn the_breakout_oscillator_dac_matches_libusdr() {
    let mut options = Exm2peOptions::default();
    options.gps = Some(true);
    let selection = FrontEndSelection::Exm2pe(options);
    let kind = assert_open_matches(with_exm2pe, c"fe=exm2pe:gps_on", selection).expect("both open");
    assert_eq!(kind, Some(FrontEndKind::Exm2pe { dac: true }));
    let (_, board) = assert_steps_match(
        with_exm2pe,
        c"fe=exm2pe:gps_on",
        selection,
        &[dac_step(0xbeef)],
    );
    assert_eq!(board.front_end_dac_code(), Some(0xbeef));
}

/// Without a DAC to drive, both refuse and touch nothing; libusdr with `-EACCES` for the
/// DAC-less breakout the bare board probes to.
///
/// Not compared: no front end at all (`fe=none`). libusdr's probe then returns before
/// setting its front-end pointer, which `malloc` left uninitialised, so `dac_vctcxo` reads
/// garbage (it crashes the oracle). rsl-usdr refuses; `tests/frontend.rs` covers it.
#[test]
fn no_oscillator_dac_is_refused_without_bus_access() {
    let (results, _) = assert_steps_match(bare, c"", FrontEndSelection::Auto, &[dac_step(1)]);
    assert!(
        matches!(results[..], [Err(Error::NoOscillatorDac)]),
        "{results:?}"
    );
}

/// `PCIe` front-end commands need that board: elsewhere both refuse without bus access.
#[test]
fn pciefe_commands_need_a_pciefe() {
    let step = pciefe_step(0x7, 1, PciefeSetting::Lna(true));
    let (results, _) = assert_steps_match(bare, c"", FrontEndSelection::Auto, &[step]);
    assert!(
        matches!(results[..], [Err(Error::FrontEndMissing("pciefe"))]),
        "{results:?}"
    );
}

/// The automatic probe meets the `PCIe` front end first. Its expanders, absent here, read
/// the idle bus, which passes its checks, and the breakout's DAC then answers as the front
/// end's: libusdr, and rsl-usdr with it, take a breakout with a DAC for a `PCIe` front end.
#[test]
fn a_lone_breakout_dac_passes_for_a_pciefe() {
    let kind = assert_open_matches(with_exm2pe, c"", FrontEndSelection::Auto).expect("both open");
    assert_eq!(kind, Some(FrontEndKind::Pciefe));
}

/// A board with the `SuperSync` clock board attached.
fn with_supersync() -> SimBoard {
    SimBoard::new(REVISION).with_front_end(FrontEnd::SuperSync)
}

/// The automatic probe passes the absent `PCIe` front end and XMASS, then loads
/// `SuperSync`'s clock synchronizer; so does a hint.
#[test]
fn supersync_comes_up_like_libusdr() {
    for (parameters, selection) in [
        (c"", FrontEndSelection::Auto),
        (c"fe=supersync", FrontEndSelection::SuperSync),
    ] {
        let kind = assert_open_matches(with_supersync, parameters, selection).expect("both open");
        assert_eq!(kind, Some(FrontEndKind::SuperSync), "{parameters:?}");
    }
}

/// `SuperSync` has no oscillator DAC (libusdr's `-EINVAL`): refused without bus access.
#[test]
fn supersync_has_no_oscillator_dac() {
    let (results, board) = assert_steps_match(
        with_supersync,
        c"fe=supersync",
        FrontEndSelection::SuperSync,
        &[dac_step(1)],
    );
    assert!(
        matches!(results[..], [Err(Error::NoOscillatorDac)]),
        "{results:?}"
    );
    // The register load's last entry landed.
    assert_eq!(board.front_end_clock_reg(0x0507), Some(0x02));
}

/// Intentional divergence, scoped to these scenarios: when XMASS or `SimpleSync` answers,
/// rsl-usdr stops the open (their LMK05318 bring-up is not ported) where libusdr carries
/// on. Up to that point the traces must agree, so rsl-usdr's whole trace is a prefix of
/// libusdr's and libusdr went on to talk to the board.
#[test]
fn answering_clock_boards_stop_the_open_where_libusdr_carries_on() {
    for (board, parameters, selection, name) in [
        (FrontEnd::Xmass, c"", FrontEndSelection::Auto, "xmass"),
        (
            FrontEnd::Xmass,
            c"fe=xmass",
            FrontEndSelection::Xmass,
            "xmass",
        ),
        (
            FrontEnd::SimpleSync,
            c"",
            FrontEndSelection::Auto,
            "simplesync",
        ),
        (
            FrontEnd::SimpleSync,
            c"fe=simplesync",
            FrontEndSelection::SimpleSync,
            "simplesync",
        ),
    ] {
        let make = || SimBoard::new(REVISION).with_front_end(board);
        // libusdr's lock waits run on the wall clock against a model that never locks, so
        // its trace past detection is not deterministic: only the prefix is compared.
        let reference = match Oracle::open_with(make(), parameters) {
            Ok(oracle) => oracle.close(),
            Err(error) => *error.board,
        };
        let sim = SimBus::new(make());
        let result = Device::builder(sim.clone()).front_end(selection).open();
        assert!(
            matches!(result, Err(Error::UnsupportedFrontEnd(found)) if found == name),
            "{parameters:?}: {result:?}"
        );
        let actual = without_thermal_gate(sim.board().trace(), REVISION);
        let actual = without_failure_power_down(&actual).to_vec();
        let expected = reference.trace();
        assert!(
            expected.len() > actual.len(),
            "{parameters:?}: libusdr carries on"
        );
        assert_same_ops(
            &expected[..actual.len()],
            &actual,
            &format!("{parameters:?}"),
        );
    }
}

/// A V0 front end whose DAC reads 0x0000 counts as no board at all, hinted or not: libusdr
/// passes `dac80501_init`'s `-ENODEV` through. A V1 board fails the open instead.
#[test]
fn a_dac_that_reads_absent_unmakes_a_v0_pciefe() {
    let make = || with_pciefe().with_front_end_dac_id(0x0000);
    let mut options = PciefeOptions::default();
    options.revision = PciefeRevision::V0;
    let kind = assert_open_matches(make, c"fe=pciefev0", FrontEndSelection::Pciefe(options))
        .expect("open");
    assert_eq!(kind, None);
    let result = assert_open_matches(
        make,
        c"fe=pciefe:v1",
        FrontEndSelection::Pciefe(PciefeOptions::default()),
    );
    assert!(
        matches!(
            result,
            Err(Error::FrontEnd {
                board: "pciefe",
                ..
            })
        ),
        "{result:?}"
    );
}

/// An expander whose inputs read all low is dead: the automatic probe skips the board and
/// goes on, here to the breakout, whose DAC (the front end's) answers; a hint fails the open.
#[test]
fn a_dead_pciefe_expander_is_skipped_or_fails_the_hint() {
    let make = || with_pciefe().with_dead_front_end_expander();
    let kind = assert_open_matches(make, c"", FrontEndSelection::Auto).expect("both open");
    assert_eq!(kind, Some(FrontEndKind::Exm2pe { dac: true }));
    let result = assert_open_matches(
        make,
        c"fe=pciefe:v1",
        FrontEndSelection::Pciefe(PciefeOptions::default()),
    );
    assert!(
        matches!(
            result,
            Err(Error::FrontEnd {
                board: "pciefe",
                ..
            })
        ),
        "{result:?}"
    );
}
