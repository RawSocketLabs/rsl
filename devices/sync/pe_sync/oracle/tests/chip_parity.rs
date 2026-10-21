//! The clock chips' solvers and register sequences in pe-sync against libusdr's
//! compiled `hw/lmx2820`, `hw/lmx1214` and `hw/lmx1204` drivers, word for word, over sweeps
//! of requests well beyond `PE_SYNC`'s one operating point: every reference path, VCO core,
//! output divider and mux of the LMX2820, its SYSREF chains, instant calibration and phase
//! sync; buffer and divider modes and every auxiliary path of the LMX1214, in and out of
//! precision mode, and its SYNC windowing; buffer, divider and multiplier modes, logic
//! dividers and SYSREF modes of the LMX1204, in and out of precision mode, its SYSREF delays
//! and windowing; and the requests each solver refuses, which both refuse before any bus
//! access.
//!
//! Not compared: the windowing captures' success path. libusdr's `common_ti_calc_sync_delay`
//! reads flags it never wrote (uninitialised stack) whenever it finds a clock edge, so its
//! result is undefined; the sim's LMX1214 positions (0) are refused by both before that, and
//! pe-sync's reading is tested on its own (`ti_sync`).
//!
//! The driver keeps these sequences private, so the test compiles its `src/portable` tree
//! directly, as its `no_std` witness does.

#[expect(
    dead_code,
    reason = "the board sequences and helpers the chip sweeps do not use"
)]
#[path = "../../src/portable/mod.rs"]
mod portable;
#[path = "../../tests/common/sim_bus.rs"]
mod sim_bus;

use std::fmt::Debug;

use bnb::{u2, u7};
use hal::{Sequence, run_sequence};
use pe_sync::lowlevel::SpiAddr;
use pe_sync_oracle::{
    AFTER_SYNC, BEFORE_SYNC, CAPTURE, Lmx1204Delay, Lmx1204Request as CLmx1204,
    Lmx1214Request as CLmx1214, Lmx2820Session, RELOAD_DELAYS, lmx1204_solver, lmx1214_solver,
    lmx2820_session,
};
use pe_sync_sim::{Op, SimBoard};
use portable::lmx1204::plan::{Channel, Delay, Plan as Lmx1204Plan, Request as Lmx1204Request};
use portable::lmx1204::reg::{Format, SysrefModeCode};
use portable::lmx1204::sequence::{WaitLock, configure as lmx1204_configure};
use portable::lmx1204::windowing as lmx1204_windowing;
use portable::lmx1214::plan::{Aux, Plan as Lmx1214Plan, Request as Lmx1214Request};
use portable::lmx1214::reg::AuxFormat;
use portable::lmx1214::sequence::configure as lmx1214_configure;
use portable::lmx1214::windowing as lmx1214_windowing;
use portable::lmx2820::controls::{InstantTune, sync};
use portable::lmx2820::plan::{
    Plan as Lmx2820Plan, Request as Lmx2820Request, RequestFormat, SysrefRequest, SysrefSource,
};
use portable::lmx2820::reg::MashOrder;
use portable::lmx2820::sequence::Tune;
use sim_bus::SimBus;

/// `EINVAL`: every solver's refusal.
const EINVAL: i32 = 22;
/// `ETIMEDOUT`: a lock wait's.
const ETIMEDOUT: i32 = 110;

/// A frequency as the `double` libusdr takes; the sweeps' are below 2^53, so exact.
#[expect(
    clippy::cast_precision_loss,
    reason = "the swept frequencies are below 2^53"
)]
fn hz(value: u64) -> f64 {
    value as f64
}

/// Runs `sequence` to its end on `sim`, whose bus never fails.
fn run_on(sim: &mut SimBus, sequence: &mut impl Sequence) {
    run_sequence(sim, sequence).expect("the sim's bus never fails");
}

/// Panics, naming `request`, where libusdr and pe-sync differ in outcome or traffic.
fn assert_same(
    request: &dyn Debug,
    (libusdr_result, libusdr): (i32, &[Op]),
    (result, ours): (i32, &[Op]),
) {
    assert_eq!(
        libusdr_result, result,
        "{request:?}: libusdr's outcome (left) differs"
    );
    if let Some(at) = (0..libusdr.len().max(ours.len())).find(|&i| libusdr.get(i) != ours.get(i)) {
        panic!(
            "{request:?}: diverges at op {at}: libusdr {:?}, pe-sync {:?}",
            libusdr.get(at),
            ours.get(at)
        );
    }
}

/// Frequencies around the LMX2820's limits and its VCO cores, in Hz.
const LMX2820_OUTPUTS: [u64; 17] = [
    44_999_999,
    45_000_000,
    100_000_000,
    550_000_000,
    1_000_000_000,
    1_100_000_000,
    2_500_000_000,
    5_649_999_999,
    5_650_000_000,
    6_000_000_000,
    7_000_000_000,
    8_800_000_000,
    11_300_000_000,
    11_300_000_001,
    16_000_000_000,
    22_600_000_000,
    22_600_000_001,
];

/// References from below the range through the doubler, multiplier and divider paths.
const LMX2820_REFERENCES: [u64; 15] = [
    4_999_999,
    5_000_000,
    10_000_000,
    25_000_000,
    40_000_000,
    61_440_000,
    100_000_000,
    122_880_000,
    150_000_000,
    250_000_000,
    300_000_000,
    500_000_000,
    1_000_000_000,
    1_400_000_000,
    1_400_000_001,
];

#[test]
fn lmx2820_tunes_match_libusdr_across_references_orders_and_outputs() {
    let mash_orders = [
        MashOrder::Integer,
        MashOrder::First,
        MashOrder::Second,
        MashOrder::Third,
    ];
    let mut solved = 0;
    for reference in LMX2820_REFERENCES {
        for mash in mash_orders {
            for multiplier in [0_u8, 3, 7, 9] {
                for a in LMX2820_OUTPUTS {
                    for b in [a, a / 2, a / 4, a / 128, a / 256, a * 2, a + 1] {
                        let request = Lmx2820Request {
                            reference_hz: reference,
                            mash,
                            multiplier: (multiplier != 0).then_some(multiplier),
                            out_a_hz: a,
                            out_b_hz: b,
                            sysref: None,
                        };
                        let reached = compare_lmx2820(&request, Lmx2820Then::default());
                        solved += u32::from(reached > Reached::Refused);
                    }
                }
            }
        }
    }
    // 7849 of the sweep solve when written; the floor keeps it from testing refusals only.
    assert!(
        solved > 5000,
        "the sweep reaches the solver's successes: {solved}"
    );
}

/// What an LMX2820 session does after its tune.
#[derive(Clone, Copy, Debug, Default)]
struct Lmx2820Then {
    /// Instant calibration's set-up instead of a tune.
    instant: bool,
    /// Then an instant retune to these outputs.
    retune: Option<(u64, u64)>,
    /// Then a phase-sync pulse.
    sync: bool,
}

/// How far an LMX2820 session got in pe-sync (and so, compared, in libusdr).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Reached {
    /// The tune (or set-up) was refused.
    Refused,
    /// The tune locked, but a later step was refused.
    Tuned,
    /// Every step succeeded.
    Done,
}

/// Runs one LMX2820 session in both drivers on fresh boards and compares outcome and
/// traffic; how far pe-sync got.
fn compare_lmx2820(request: &Lmx2820Request, then: Lmx2820Then) -> Reached {
    let sysref = request.sysref;
    let (master_mode, srreq_fmt) = match sysref.map(|sysref| sysref.source) {
        Some(SysrefSource::Repeater(format)) => (0, format as u32),
        _ => (1, 0),
    };
    let (retune_a, retune_b) = then.retune.unwrap_or_default();
    let session = Lmx2820Session {
        osc_in: request.reference_hz,
        rfouta: request.out_a_hz,
        rfoutb: request.out_b_hz,
        srout: sysref.map_or(0, |sysref| sysref.out_hz),
        retune_a,
        retune_b,
        bus: 2,
        mash_order: request.mash as u32,
        force_mult: request.multiplier.map_or(0, u32::from),
        sysref_enabled: sysref.is_some().into(),
        master_mode,
        cont_pulse: sysref.is_some_and(|sysref| sysref.pulses.is_none()).into(),
        pulse_cnt: sysref.and_then(|sysref| sysref.pulses).map_or(0, u32::from),
        delay_ctrl: sysref.map_or(0, |sysref| sysref.delay_step.into()),
        srreq_fmt,
        instant: then.instant.into(),
        retune: then.retune.is_some().into(),
        sync: then.sync.into(),
    };
    let (c_result, board) = lmx2820_session(SimBoard::new(), &session);
    let label = (request, then);
    let solved = if then.instant {
        Lmx2820Plan::for_instant_calibration(request)
    } else {
        Lmx2820Plan::solve(request)
    };
    let Ok(plan) = solved else {
        assert_same(&label, (c_result, board.trace()), (-EINVAL, &[]));
        return Reached::Refused;
    };
    let mut sim = SimBus::new(SimBoard::new());
    let target = SpiAddr(2);
    let mut tune = Tune::new(target, &plan);
    run_on(&mut sim, &mut tune);
    let result = if !tune.locked() {
        -ETIMEDOUT
    } else if let Some((a, b)) = then.retune {
        plan.retune(a, b).map_or(-EINVAL, |retuned| {
            run_on(&mut sim, &mut InstantTune::new(target, &retuned));
            0
        })
    } else {
        0
    };
    if result == 0 && then.sync {
        run_on(&mut sim, &mut sync(target));
    }
    assert_same(
        &label,
        (c_result, board.trace()),
        (result, sim.board().trace()),
    );
    if result == 0 {
        Reached::Done
    } else {
        Reached::Tuned
    }
}

#[test]
fn lmx2820_sysref_chains_match_libusdr() {
    // Each SROUT source, pulse count and delay step about their limits and the JESD
    // table's corners, over VCOs in every pre-divider band and a reference above SYSREF's.
    let sources = [
        SysrefSource::Master,
        SysrefSource::Repeater(RequestFormat::Cmos),
        SysrefSource::Repeater(RequestFormat::CmosAc),
        SysrefSource::Repeater(RequestFormat::LvdsAc),
        SysrefSource::Repeater(RequestFormat::LvdsDc),
    ];
    let pulses = [None, Some(0), Some(1), Some(7), Some(15), Some(16)];
    let delays = [0, 1, 62, 63, 64, 126, 127, 189, 190, 251, 252];
    // libusdr divides by a 0 Hz SROUT; none is swept.
    let srouts = [25_000_000, 3_125_000, 1_000_000, 100_000, 1, 200_000_000];
    let tunes = [
        (150_000_000, 1_000_000_000, 1_000_000_000),
        (150_000_000, 550_000_000, 550_000_000),
        (100_000_000, 6_000_000_000, 6_000_000_000),
        (25_000_000, 11_000_000_000, 5_500_000_000),
        (10_000_000, 20_000_000_000, 20_000_000_000),
        (250_000_000, 2_500_000_000, 2_500_000_000),
    ];
    let (mut solved, mut synced) = (0, 0);
    for (reference_hz, out_a_hz, out_b_hz) in tunes {
        for source in sources {
            for pulses in pulses {
                for delay_step in delays {
                    for out_hz in srouts {
                        let request = Lmx2820Request {
                            reference_hz,
                            mash: MashOrder::Second,
                            multiplier: None,
                            out_a_hz,
                            out_b_hz,
                            sysref: Some(SysrefRequest {
                                out_hz,
                                source,
                                pulses,
                                delay_step,
                            }),
                        };
                        let then = Lmx2820Then {
                            sync: delay_step == 0,
                            ..Lmx2820Then::default()
                        };
                        let reached = compare_lmx2820(&request, then);
                        solved += u32::from(reached > Reached::Refused);
                        synced += u32::from(then.sync && reached == Reached::Done);
                    }
                }
            }
        }
    }
    // 3600 solve and 360 phase-synced sessions complete when written; the floors keep it
    // from testing refusals only.
    assert!(
        solved > 3000,
        "the sweep reaches SYSREF's successes: {solved}"
    );
    assert!(synced > 300, "the sweep reaches the phase sync: {synced}");
}

#[test]
fn lmx2820_instant_calibration_and_retunes_match_libusdr() {
    // Set-ups over every reference path and modulator order, each retuned within, at and
    // beyond the outputs its phase detector reaches, then phase-synced.
    let retunes = [
        (5_650_000_000, 5_650_000_000),
        (6_000_000_000, 6_000_000_000),
        (8_800_000_000, 4_400_000_000),
        (1_000_000_000, 1_000_000_000),
        (550_000_000, 1_100_000_000),
        (22_600_000_000, 11_300_000_000),
        (45_000_000, 45_000_000),
        (44_999_999, 44_999_999),
        (7_000_000_001, 7_000_000_001),
        (1_000_000_000, 300_000_000),
    ];
    let (mut solved, mut completed) = (0, 0);
    for reference_hz in [
        10_000_000,
        25_000_000,
        100_000_000,
        150_000_000,
        500_000_000,
    ] {
        for mash in [
            MashOrder::Integer,
            MashOrder::First,
            MashOrder::Second,
            MashOrder::Third,
        ] {
            for multiplier in [None, Some(3)] {
                for retune in retunes {
                    let request = Lmx2820Request {
                        reference_hz,
                        mash,
                        multiplier,
                        out_a_hz: 0,
                        out_b_hz: 0,
                        sysref: None,
                    };
                    let then = Lmx2820Then {
                        instant: true,
                        retune: Some(retune),
                        sync: true,
                    };
                    let reached = compare_lmx2820(&request, then);
                    solved += u32::from(reached > Reached::Refused);
                    completed += u32::from(reached == Reached::Done);
                }
            }
        }
    }
    // 250 set-ups solve and 198 sessions retune and sync when written; the floors keep it
    // from testing refusals only.
    assert!(
        solved > 200,
        "the sweep reaches the set-up's successes: {solved}"
    );
    assert!(
        completed > 150,
        "the sweep reaches the retunes: {completed}"
    );
}

#[test]
fn lmx1214_solutions_match_libusdr_across_modes_and_auxiliary_paths() {
    let inputs = [
        299_999_999,
        300_000_000,
        550_000_000,
        1_000_000_000,
        2_200_000_000,
        3_200_000_000,
        4_400_000_000,
        6_400_000_000,
        8_000_000_000,
        12_800_000_000,
        12_800_000_001,
        18_000_000_000,
        18_000_000_001,
    ];
    let mut solved = 0;
    for input in inputs {
        let outputs_hz = [
            input,
            input / 2,
            input / 3,
            input / 8,
            input / 9,
            input / 2 + 1,
            input / 2 + 2,
            150_000_000,
            149_999_999,
            6_400_000_000,
        ];
        let aux_hz = [
            input,
            input / 2,
            input / 4,
            input / 4 / 1023,
            input / 4 / 1024,
            100_000_000,
            1_000_000,
            999_999,
            800_000_000,
            800_000_001,
        ];
        for output in outputs_hz {
            for outputs in [0b1111_u32, 0b0011, 0b0111, 0b0000, 0b1100] {
                let auxes = aux_hz
                    .iter()
                    .flat_map(|&hz| [(true, hz, AuxFormat::Lvds), (true, hz, AuxFormat::Cml)])
                    .chain([(false, 0, AuxFormat::Lvds), (false, 0, AuxFormat::Cml)]);
                for ((enabled, aux_hz, format), precision) in
                    auxes.flat_map(|aux| [(aux, false), (aux, true)])
                {
                    let request = Lmx1214Request {
                        input_hz: input,
                        output_hz: output,
                        outputs: [0, 1, 2, 3].map(|bit| outputs >> bit & 1 == 1),
                        aux: Aux {
                            enabled,
                            hz: aux_hz,
                            format,
                        },
                        precision,
                    };
                    solved += u32::from(compare_lmx1214(&request, 0));
                }
            }
        }
    }
    // 2144 solve when written; the floor keeps it from testing refusals only.
    assert!(
        solved > 1800,
        "the sweep reaches the solver's successes: {solved}"
    );
}

/// Runs one LMX1214 request in both drivers on fresh boards, then the windowing steps
/// `windowing` asks for, and compares outcome and traffic; whether pe-sync solved it.
fn compare_lmx1214(request: &Lmx1214Request, windowing: u32) -> bool {
    let outputs = (0..)
        .zip(request.outputs)
        .fold(0, |mask, (bit, on)| mask | u32::from(on) << bit);
    let c_request = CLmx1214 {
        input: request.input_hz,
        output: request.output_hz,
        outputs,
        aux_enable: request.aux.enabled.into(),
        aux_format: request.aux.format as u32,
        prec_mode: request.precision.into(),
        windowing,
        aux_freq: hz(request.aux.hz),
    };
    let (c_result, board) = lmx1214_solver(SimBoard::new(), &c_request);
    let label = (request, windowing);
    let Ok(plan) = Lmx1214Plan::solve(request) else {
        assert_same(&label, (c_result, board.trace()), (-EINVAL, &[]));
        return false;
    };
    let target = SpiAddr(5);
    let mut sim = SimBus::new(SimBoard::new());
    run_on(&mut sim, &mut lmx1214_configure(target, &plan));
    if windowing & BEFORE_SYNC != 0 {
        run_on(&mut sim, &mut lmx1214_windowing::before_sync(target, &plan));
    }
    let mut result = 0;
    if windowing & CAPTURE != 0 {
        let mut capture = lmx1214_windowing::capture(target);
        run_on(&mut sim, &mut capture);
        if capture.delay().is_none() {
            result = -EINVAL;
        }
    }
    if result == 0 && windowing & AFTER_SYNC != 0 {
        run_on(&mut sim, &mut lmx1214_windowing::after_sync(target, &plan));
    }
    assert_same(
        &label,
        (c_result, board.trace()),
        (result, sim.board().trace()),
    );
    true
}

#[test]
fn lmx1214_sync_windowing_matches_libusdr() {
    // Inputs in every delay step band and at their edges, buffered and divided, with the
    // auxiliary divider bypassed, used and off; armed and disarmed around no capture, and
    // around a capture of the sim's positions, which both refuse.
    let inputs = [
        550_000_000,
        2_400_000_000,
        2_400_000_001,
        3_100_000_001,
        4_500_000_001,
        4_700_000_001,
        5_700_000_001,
        12_800_000_000,
    ];
    let mut solved = 0;
    for input in inputs {
        for output in [input, input / 2, input / 8] {
            for aux in [
                Aux {
                    enabled: true,
                    hz: input,
                    format: AuxFormat::Lvds,
                },
                Aux {
                    enabled: true,
                    hz: 100_000_000,
                    format: AuxFormat::Cml,
                },
                Aux {
                    enabled: false,
                    hz: 0,
                    format: AuxFormat::Lvds,
                },
            ] {
                let request = Lmx1214Request {
                    input_hz: input,
                    output_hz: output,
                    outputs: [true; 4],
                    aux,
                    precision: false,
                };
                for windowing in [BEFORE_SYNC | AFTER_SYNC, BEFORE_SYNC | CAPTURE | AFTER_SYNC] {
                    solved += u32::from(compare_lmx1214(&request, windowing));
                }
            }
        }
    }
    // 68 solve when written; the floor keeps it from testing refusals only.
    assert!(solved > 50, "the sweep reaches the windowing: {solved}");
}

/// One LMX1204 SYSREF setup: on, its mode, its output and the request input.
type SysrefSetup = (bool, SysrefModeCode, u64, u64);

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the sweep's tables read best beside the loops over them"
)]
fn lmx1204_solutions_and_lock_waits_match_libusdr_across_modes() {
    let clkins: [u64; 13] = [
        299_999_999,
        300_000_000,
        500_000_000,
        1_000_000_000,
        1_600_000_000,
        2_000_000_000,
        3_200_000_000,
        4_000_000_000,
        6_400_000_000,
        8_000_000_000,
        12_800_000_000,
        12_800_000_001,
        // 2^33: a 1 Hz SYSREF needs a divider of 2^32, which C's `(unsigned)` wraps to 0.
        8_589_934_592,
    ];
    // libusdr divides by a pulser's or generator's zero-hertz output; none is swept.
    let sysrefs: [SysrefSetup; 9] = [
        (false, SysrefModeCode::Continuous, 0, 0),
        (true, SysrefModeCode::Continuous, 3_125_000, 25_000_000),
        (true, SysrefModeCode::Continuous, 25_000_000, 0),
        (true, SysrefModeCode::Continuous, 201_000_000, 0),
        (true, SysrefModeCode::Pulser, 1_000_000, 0),
        (true, SysrefModeCode::Repeater, 10_000_000, 10_000_000),
        (true, SysrefModeCode::Repeater, 10_000_000, 20_000_000),
        (true, SysrefModeCode::Continuous, 1, 0),
        // No pre-divider leaves a divider within 4095: libusdr carries on past the last.
        (true, SysrefModeCode::Continuous, 50_000, 0),
    ];
    let all = Channel {
        enabled: true,
        clock: true,
        sysref: true,
    };
    let mut solved = 0;
    for clkin in clkins {
        let clkouts = [
            clkin,
            clkin * 2,
            clkin * 3,
            clkin * 4,
            clkin * 5,
            clkin / 2,
            clkin / 8,
            clkin / 9,
            3_200_000_000,
            6_400_000_000,
        ];
        let logic_hz = [
            clkin,
            125_000_000,
            1_000_000,
            800_000_000,
            999_999,
            333_000_000,
        ];
        for clkout in clkouts {
            for filter in [false, true] {
                for (sysref, mode, sysref_out, sysref_request) in sysrefs {
                    for (logic, precision) in logic_hz
                        .into_iter()
                        .flat_map(|logic| [(logic, false), (logic, true)])
                    {
                        let request = Lmx1204Request {
                            clkin_hz: clkin,
                            sysref_request_hz: sysref_request,
                            filter,
                            sysref_mode: mode,
                            sysref,
                            channels: [all; 5],
                            clkout_hz: clkout,
                            sysref_out_hz: sysref_out,
                            logic_hz: logic,
                            logic_clock_format: Format::Lvds,
                            logic_sysref_format: Format::Cml,
                            delays: [Delay::CREATE; 5],
                            precision,
                        };
                        solved += compare_lmx1204(&request, 0b1_1111, 0b1_1111, 0b1_1111);
                    }
                }
            }
        }
    }
    // Channel and format switches on the board's own plan.
    for (ch_en, clkout_en, sysrefout_en) in [
        (0, 0, 0),
        (0b0_1111, 0b1_0101, 0b0_1010),
        (0b1_0000, 0b1_0000, 0),
    ] {
        for format in [Format::Lvds, Format::Lvpecl, Format::Cml] {
            let bit = |mask: u32, n: u32| mask >> n & 1 == 1;
            let channels = [0, 1, 2, 3, 4].map(|n| Channel {
                enabled: bit(ch_en, n),
                clock: bit(clkout_en, n),
                sysref: bit(sysrefout_en, n),
            });
            let request = Lmx1204Request {
                clkin_hz: 1_000_000_000,
                sysref_request_hz: 25_000_000,
                filter: false,
                sysref_mode: SysrefModeCode::Continuous,
                sysref: true,
                channels,
                clkout_hz: 4_000_000_000,
                sysref_out_hz: 3_125_000,
                logic_hz: 125_000_000,
                logic_clock_format: format,
                logic_sysref_format: format,
                delays: [Delay::CREATE; 5],
                precision: false,
            };
            solved += compare_lmx1204(&request, ch_en, clkout_en, sysrefout_en);
        }
    }
    // 1451 solve when written; the floor keeps it from testing refusals only.
    assert!(
        solved > 1200,
        "the sweep reaches the solver's successes: {solved}"
    );
}

/// Compares one LMX1204 request, whose channel switches are also given as libusdr's masks;
/// 1 when pe-sync solved it.
fn compare_lmx1204(request: &Lmx1204Request, ch_en: u32, clkout_en: u32, sysrefout_en: u32) -> u32 {
    let masks = (ch_en, clkout_en, sysrefout_en);
    u32::from(compare_lmx1204_session(request, masks, 0, request.delays))
}

/// libusdr's form of a delay.
fn c_delay(delay: Delay) -> Lmx1204Delay {
    Lmx1204Delay {
        phase: delay.phase.value().into(),
        i: delay.i.value().into(),
        q: delay.q.value().into(),
    }
}

/// Runs one LMX1204 request in both drivers on fresh boards (its channel switches also as
/// libusdr's masks), then the steps `windowing` asks for (`reload` the delays it reloads),
/// and compares outcome and traffic; whether pe-sync solved it.
fn compare_lmx1204_session(
    request: &Lmx1204Request,
    (ch_en, clkout_en, sysrefout_en): (u32, u32, u32),
    windowing: u32,
    reload: [Delay; 5],
) -> bool {
    let c_request = CLmx1204 {
        clkin: request.clkin_hz,
        sysrefreq: request.sysref_request_hz,
        filter_mode: request.filter.into(),
        sysref_mode: request.sysref_mode as u32,
        sysref_en: request.sysref.into(),
        ch_en,
        clkout_en,
        sysrefout_en,
        logiclkout_fmt: request.logic_clock_format as u32,
        logisysrefout_fmt: request.logic_sysref_format as u32,
        prec_mode: request.precision.into(),
        windowing,
        clkout: hz(request.clkout_hz),
        sysrefout: hz(request.sysref_out_hz),
        logiclkout: hz(request.logic_hz),
        delays: request.delays.map(c_delay),
        reload: reload.map(c_delay),
    };
    let (c_result, board) = lmx1204_solver(SimBoard::new(), &c_request);
    let label = (request, windowing, reload);
    let Ok(plan) = Lmx1204Plan::solve(request) else {
        assert_same(&label, (c_result, board.trace()), (-EINVAL, &[]));
        return false;
    };
    let target = SpiAddr(4);
    let mut sim = SimBus::new(SimBoard::new());
    run_on(&mut sim, &mut lmx1204_configure(target, &plan));
    let mut wait = WaitLock::new(target, &plan);
    run_on(&mut sim, &mut wait);
    let result = if wait.locked() { 0 } else { -ETIMEDOUT };
    let plan = plan.with_delays(reload);
    if result == 0 && windowing & RELOAD_DELAYS != 0 {
        run_on(
            &mut sim,
            &mut lmx1204_windowing::reload_delays(target, &plan),
        );
    }
    if result == 0 && windowing & BEFORE_SYNC != 0 {
        run_on(&mut sim, &mut lmx1204_windowing::before_sync(target, &plan));
    }
    if result == 0 && windowing & AFTER_SYNC != 0 {
        run_on(&mut sim, &mut lmx1204_windowing::after_sync(target, &plan));
    }
    assert_same(
        &label,
        (c_result, board.trace()),
        (result, sim.board().trace()),
    );
    true
}

/// A delay from its phase, I and Q.
fn delay(phase: u8, i: u8, q: u8) -> Delay {
    Delay {
        phase: u2::new(phase),
        i: u7::new(i),
        q: u7::new(q),
    }
}

#[test]
fn lmx1204_sysref_delays_and_windowing_match_libusdr() {
    // Per-output delays set before the solver and reloaded after it, then the SYSREF
    // windowing armed and disarmed, over CLKINs in every delay step band and at their
    // edges, with SYSREF on and off and the logic divider bypassed and used. An enabled
    // channel whose I and Q do not sum to 0x7F is refused with SYSREF on; a disabled one is
    // not checked, and the reload checks none.
    let custom = [
        delay(0, 0, 0x7f),
        delay(1, 0x7f, 0),
        delay(2, 0x40, 0x3f),
        delay(3, 1, 0x7e),
        delay(1, 7, 0x78),
    ];
    let reloaded = [
        delay(3, 0x7f, 0x7f),
        delay(0, 0, 0),
        delay(1, 0x11, 0x22),
        delay(2, 0x33, 0x44),
        delay(0, 0x55, 0x66),
    ];
    let unpaired = [
        Delay::CREATE,
        delay(1, 0x11, 0x22),
        Delay::CREATE,
        Delay::CREATE,
        Delay::CREATE,
    ];
    let all = Channel {
        enabled: true,
        clock: true,
        sysref: true,
    };
    let mut ch1_off = [all; 5];
    ch1_off[1] = Channel::default();
    let cases = [
        ([all; 5], [Delay::CREATE; 5], reloaded),
        ([all; 5], custom, custom),
        ([all; 5], unpaired, reloaded),
        (ch1_off, unpaired, reloaded),
    ];
    let mut solved = 0;
    for clkin in [
        800_000_000,
        1_000_000_000,
        2_400_000_000,
        2_400_000_001,
        3_100_000_001,
        4_500_000_001,
        4_700_000_001,
        5_700_000_001,
        12_800_000_000,
    ] {
        for (clkout, logic_hz) in [
            (clkin, clkin),
            (clkin, 100_000_000),
            (clkin / 2, 100_000_000),
        ] {
            for (sysref, sysref_out_hz) in [(true, 3_125_000), (false, 0)] {
                for (channels, delays, reload) in cases {
                    let request = Lmx1204Request {
                        clkin_hz: clkin,
                        sysref_request_hz: 0,
                        filter: false,
                        sysref_mode: SysrefModeCode::Continuous,
                        sysref,
                        channels,
                        clkout_hz: clkout,
                        sysref_out_hz,
                        logic_hz,
                        logic_clock_format: Format::Lvds,
                        logic_sysref_format: Format::Lvds,
                        delays,
                        precision: false,
                    };
                    let windowing = RELOAD_DELAYS | BEFORE_SYNC | AFTER_SYNC;
                    let mask = |pick: fn(&Channel) -> bool| {
                        (0..).zip(channels).fold(0, |mask, (bit, channel)| {
                            mask | u32::from(pick(&channel)) << bit
                        })
                    };
                    let masks = (
                        mask(|channel| channel.enabled),
                        mask(|channel| channel.clock),
                        mask(|channel| channel.sysref),
                    );
                    solved +=
                        u32::from(compare_lmx1204_session(&request, masks, windowing, reload));
                }
            }
        }
    }
    // 71 solve when written; the floor keeps it from testing refusals only.
    assert!(solved > 50, "the sweep reaches the windowing: {solved}");
}
