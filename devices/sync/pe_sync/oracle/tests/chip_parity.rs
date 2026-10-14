//! The clock chips' solvers and register sequences in pe-sync against libusdr's
//! compiled `hw/lmx2820`, `hw/lmx1214` and `hw/lmx1204` drivers, word for word, over sweeps
//! of requests well beyond `PE_SYNC`'s one operating point: every reference path, VCO core,
//! output divider and mux of the LMX2820; buffer and divider modes and every auxiliary path
//! of the LMX1214; buffer, divider and multiplier modes, logic dividers and SYSREF modes of
//! the LMX1204; and the requests each solver refuses, which both refuse before any bus
//! access.
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

use pe_sync::lowlevel::SpiAddr;
use pe_sync_oracle::{
    Lmx1204Request as CLmx1204, Lmx1214Request as CLmx1214, Lmx2820Tune, lmx1204_solver,
    lmx1214_solver, lmx2820_tune,
};
use pe_sync_sim::{Op, SimBoard};
use portable::lmx1204::plan::{Channel, Plan as Lmx1204Plan, Request as Lmx1204Request};
use portable::lmx1204::reg::{Format, SysrefModeCode};
use portable::lmx1204::sequence::{WaitLock, configure as lmx1204_configure};
use portable::lmx1214::plan::{Aux, Plan as Lmx1214Plan, Request as Lmx1214Request};
use portable::lmx1214::reg::AuxFormat;
use portable::lmx1214::sequence::configure as lmx1214_configure;
use portable::lmx2820::plan::{Plan as Lmx2820Plan, Request as Lmx2820Request};
use portable::lmx2820::reg::MashOrder;
use portable::lmx2820::sequence::Tune;
use rsl_lowlevel::{Sequence, run_sequence};
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

/// Runs `sequence` on a fresh board: whether it succeeded, and the board's trace.
fn run_fresh(sequence: &mut impl Sequence) -> (bool, Vec<Op>) {
    let mut sim = SimBus::new(SimBoard::new());
    let succeeded = run_sequence(&mut sim, sequence).is_ok();
    let trace = sim.board().trace().to_vec();
    (succeeded, trace)
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
        for (code, mash) in (0..).zip(mash_orders) {
            for multiplier in [0_u8, 3, 7, 9] {
                for a in LMX2820_OUTPUTS {
                    for b in [a, a / 2, a / 4, a / 128, a / 256, a * 2, a + 1] {
                        let tune = Lmx2820Tune {
                            bus: 2,
                            osc_in: reference,
                            mash_order: code,
                            force_mult: multiplier.into(),
                            rfouta: a,
                            rfoutb: b,
                        };
                        let (c_result, board) = lmx2820_tune(SimBoard::new(), tune);
                        let request = Lmx2820Request {
                            reference_hz: reference,
                            mash,
                            multiplier: (multiplier != 0).then_some(multiplier),
                            out_a_hz: a,
                            out_b_hz: b,
                        };
                        let (result, ours) = match Lmx2820Plan::solve(&request) {
                            Err(_) => (-EINVAL, Vec::new()),
                            Ok(plan) => {
                                solved += 1;
                                let mut tune = Tune::new(SpiAddr(2), &plan);
                                let (ok, trace) = run_fresh(&mut tune);
                                assert!(ok, "{request:?}: the sim's bus never fails");
                                (if tune.locked() { 0 } else { -ETIMEDOUT }, trace)
                            }
                        };
                        assert_same(&request, (c_result, board.trace()), (result, &ours));
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
                for (enabled, aux_hz, format) in auxes {
                    let c_request = CLmx1214 {
                        input,
                        output,
                        outputs,
                        aux_enable: enabled.into(),
                        aux_format: format as u32,
                        aux_freq: hz(aux_hz),
                    };
                    let (c_result, board) = lmx1214_solver(SimBoard::new(), &c_request);
                    let request = Lmx1214Request {
                        input_hz: input,
                        output_hz: output,
                        outputs: [0, 1, 2, 3].map(|bit| outputs >> bit & 1 == 1),
                        aux: Aux {
                            enabled,
                            hz: aux_hz,
                            format,
                        },
                    };
                    let (result, ours) = match Lmx1214Plan::solve(&request) {
                        Err(_) => (-EINVAL, Vec::new()),
                        Ok(plan) => {
                            solved += 1;
                            let (ok, trace) = run_fresh(&mut lmx1214_configure(SpiAddr(5), &plan));
                            assert!(ok, "{request:?}: the sim's bus never fails");
                            (0, trace)
                        }
                    };
                    assert_same(&request, (c_result, board.trace()), (result, &ours));
                }
            }
        }
    }
    // 1392 solve when written; the floor keeps it from testing refusals only.
    assert!(
        solved > 1000,
        "the sweep reaches the solver's successes: {solved}"
    );
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
                    for logic in logic_hz {
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
            };
            solved += compare_lmx1204(&request, ch_en, clkout_en, sysrefout_en);
        }
    }
    // 599 solve when written; the floor keeps it from testing refusals only.
    assert!(
        solved > 500,
        "the sweep reaches the solver's successes: {solved}"
    );
}

/// Compares one LMX1204 request, whose channel switches are also given as libusdr's masks;
/// 1 when pe-sync solved it.
fn compare_lmx1204(request: &Lmx1204Request, ch_en: u32, clkout_en: u32, sysrefout_en: u32) -> u32 {
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
        clkout: hz(request.clkout_hz),
        sysrefout: hz(request.sysref_out_hz),
        logiclkout: hz(request.logic_hz),
    };
    let (c_result, board) = lmx1204_solver(SimBoard::new(), &c_request);
    let Ok(plan) = Lmx1204Plan::solve(request) else {
        assert_same(request, (c_result, board.trace()), (-EINVAL, &[]));
        return 0;
    };
    let mut sim = SimBus::new(SimBoard::new());
    run_sequence(&mut sim, &mut lmx1204_configure(SpiAddr(4), &plan))
        .expect("the sim's bus never fails");
    let mut wait = WaitLock::new(SpiAddr(4), &plan);
    run_sequence(&mut sim, &mut wait).expect("the sim's bus never fails");
    let result = if wait.locked() { 0 } else { -ETIMEDOUT };
    assert_same(
        request,
        (c_result, board.trace()),
        (result, sim.board().trace()),
    );
    1
}
