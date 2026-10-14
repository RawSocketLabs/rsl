//! usdr's LMK05318 frequency plans and the registers it stages for them (the shared
//! `lmk05318` crate) against libusdr's (`hw/lmk05318/lmk05318.c`), number for number
//! and bit for bit: the XO path and APLL1, the output solver with APLL2's state, and output
//! 7's divider stages. The solver's `-Ofast` arithmetic makes the float results the
//! sharpest check. Bus traces are compared by `frontend_parity.rs`.

use std::collections::BTreeSet;

use lmk05318::map::RegisterMap;
use lmk05318::plan::{
    self, Affinity, Apll2, Apll2Setting, ClockPlanError, Mux, OutputFormat, OutputRequest, Plan,
    Xo, XoInput, out7_stages,
};
use lmk05318::staging::{stage_apll1, stage_plan, stage_xo};
use usdr_oracle::lmk05318::{self as oracle, Apll2State, MAP_LEN, PortRequest, Staged};

/// Every XO input buffer with its C value (`enum xo_input_type`).
const XO_INPUTS: [(XoInput, i32); 6] = [
    (XoInput::DcDifferentialExternal, 0),
    (XoInput::AcDifferentialExternal, 1),
    (XoInput::AcDifferentialInternal100, 2),
    (XoInput::HcslInternal50, 3),
    (XoInput::Cmos, 4),
    (XoInput::SingleEndedInternal50, 5),
];

/// Every output format, in `enum lmk05318_type`'s order (its C value is the index).
const FORMATS: [OutputFormat; 16] = [
    OutputFormat::Off,
    OutputFormat::Lvds,
    OutputFormat::Cml,
    OutputFormat::Lvpecl,
    OutputFormat::HcslExternal50,
    OutputFormat::HcslInternal50,
    OutputFormat::LvcmosHizHiz,
    OutputFormat::LvcmosHizN,
    OutputFormat::LvcmosHizP,
    OutputFormat::LvcmosLowLow,
    OutputFormat::LvcmosNHiz,
    OutputFormat::LvcmosNN,
    OutputFormat::LvcmosNP,
    OutputFormat::LvcmosPHiz,
    OutputFormat::LvcmosPN,
    OutputFormat::LvcmosPP,
];

/// A deterministic generator (64-bit LCG, Knuth's constants), so failures reproduce.
struct Lcg(u64);

impl Lcg {
    /// The next 32 random bits.
    fn next(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 32) as u32
    }

    /// A value below `bound`.
    fn below(&mut self, bound: u32) -> u32 {
        self.next() % bound
    }
}

/// A big-endian number from staged registers.
fn number(regs: &Staged, addrs: &[usize]) -> u64 {
    addrs.iter().fold(0, |value, &addr| {
        value << 8 | u64::from(regs[addr].expect("staged by libusdr"))
    })
}

/// APLL2's state as the C's.
fn to_c(apll2: &Apll2) -> Apll2State {
    Apll2State {
        rp: apll2.primary_divider(),
        rs: apll2.secondary_divider(),
        vco2_freq: apll2.vco_hz(),
        n: apll2.n(),
        num: apll2.num(),
        den: apll2.den(),
        pd1: apll2.pd1(),
        pd2: apll2.pd2(),
    }
}

/// A request as the C's.
fn to_c_request(request: &OutputRequest) -> PortRequest {
    PortRequest {
        port: request.port.into(),
        freq: request.hz,
        revert: request.inverted,
        format: request.format as i32,
        affinity: request.affinity as i32,
    }
}

/// A staging map as the shim reports libusdr's: a byte or nothing per address.
fn staged(map: &RegisterMap) -> Staged {
    let mut regs = vec![None; MAP_LEN];
    for (addr, value) in map.staged() {
        regs[usize::from(addr)] = Some(value);
    }
    regs
}

/// The staged addresses where `actual` and libusdr's `expected` differ, for messages.
fn differences(actual: &Staged, expected: &Staged) -> Vec<(usize, Option<u8>, Option<u8>)> {
    (0..MAP_LEN)
        .filter(|&addr| actual[addr] != expected[addr])
        .map(|addr| (addr, actual[addr], expected[addr]))
        .collect()
}

/// Plans `requests` in both from equal APLL2 states and compares the outcome, the states
/// after (which libusdr changes even on some failures), each output's route (frequency bits
/// included) and every register staged. Returns usdr's plan.
fn assert_plans_match(apll2: &mut Apll2, requests: &[OutputRequest], what: &str) -> Option<Plan> {
    let mut c_state = to_c(apll2);
    let c_requests: Vec<PortRequest> = requests.iter().map(to_c_request).collect();
    let (c_status, c_outputs, c_regs) = oracle::solve(&mut c_state, &c_requests);
    let plan = apll2.plan(requests);
    assert_eq!(
        plan.is_ok(),
        c_status == 0,
        "{what}: outcome ({plan:?}, C {c_status})"
    );
    assert_eq!(to_c(apll2), c_state, "{what}: APLL2 state");
    let Ok(plan) = plan else {
        assert!(
            c_regs.iter().all(Option::is_none),
            "{what}: libusdr staged registers for a refused plan"
        );
        return None;
    };
    let mut map = RegisterMap::new();
    stage_plan(&mut map, apll2, &plan).expect("a plan stages");
    let regs = staged(&map);
    assert!(
        regs == c_regs,
        "{what}: staged registers (address, usdr, libusdr): {:x?}",
        differences(&regs, &c_regs)
    );
    // The C settles APLL2 into its state; so does usdr's plan, and its setting agrees.
    match plan.apll2 {
        Apll2Setting::Off => {}
        Apll2Setting::On {
            n,
            num,
            den,
            pd1,
            pd2,
            ..
        } => assert_eq!(
            (n, num, den, pd1, pd2),
            (
                c_state.n,
                c_state.num,
                c_state.den,
                c_state.pd1,
                c_state.pd2
            ),
            "{what}: APLL2 setting"
        ),
    }
    assert_eq!(plan.outputs.len(), c_outputs.len(), "{what}");
    for (output, c) in plan.outputs.iter().zip(&c_outputs) {
        assert_eq!(
            u32::from(output.request.port),
            c.port,
            "{what}: output order"
        );
        let Some(route) = output.route else {
            assert_eq!(c.freq, 0, "{what}: output {} off", c.port);
            continue;
        };
        let mux = match route.mux {
            Mux::Apll1 => 0,
            Mux::Apll1Inverted => 1,
            Mux::Apll2Post1 => 2,
            Mux::Apll2Post2 => 3,
        };
        assert_eq!(
            (mux, route.divider, route.hz.to_bits()),
            (c.mux, c.out_div, c.freq_out.to_bits()),
            "{what}: output {} route ({} Hz vs C {} Hz)",
            c.port,
            route.hz,
            c.freq_out
        );
    }
    Some(plan)
}

/// XO paths and APLL1 fractions across the XO range and every input type, with
/// libusdr's out-of-range and inexact refusals.
#[test]
fn xo_paths_and_apll1_match_libusdr() {
    let mut frequencies = vec![
        9_999_999,
        10_000_000,
        12_800_000,
        25_000_000,
        26_000_000,
        40_000_000,
        40_000_001,
        52_000_000,
        80_000_000,
        80_000_001,
        100_000_000,
        100_000_001,
    ];
    let mut lcg = Lcg(0x5318);
    frequencies.extend((0..400).map(|_| 9_000_000 + lcg.below(92_000_000)));
    frequencies.extend((0..200).map(|_| 10_000_000 + 1000 * lcg.below(90_001)));
    let mut exact = 0;
    let mut baw_tables = BTreeSet::new();
    for hz in frequencies {
        for (input, c_input) in XO_INPUTS {
            let detector_bypass = lcg.below(2) == 1;
            let xo = Xo {
                hz,
                input,
                detector_bypass,
            };
            let c = oracle::xo_apll1(hz, c_input, detector_bypass);
            let what = format!("XO {hz} Hz, {input:?}");
            let Ok(path) = xo.path() else {
                assert_eq!(c.rdiv, 0, "{what}: libusdr chose a path");
                continue;
            };
            assert_eq!((path.doubler, path.divider), (c.doubler, c.rdiv), "{what}");
            let mut map = RegisterMap::new();
            let staged_xo = stage_xo(&mut map, xo);
            assert_eq!(
                staged_xo.is_ok(),
                c.xo_res == 0,
                "{what}: BAW settings exist"
            );
            if staged_xo.is_ok() {
                baw_tables.insert(hz);
            }
            let apll1 = xo.apll1(path);
            assert_eq!(apll1.is_ok(), c.apll1_res == 0, "{what}: APLL1 ({apll1:?})");
            if let Ok(apll1) = apll1 {
                stage_apll1(&mut map, apll1);
            }
            let regs = staged(&map);
            assert!(
                regs == c.regs,
                "{what}: staged registers (address, usdr, libusdr): {:x?}",
                differences(&regs, &c.regs)
            );
            let Ok(apll1) = apll1 else {
                continue;
            };
            exact += 1;
            // The 24-bit layout: NDIV 0x6C:0x6D, numerator 0x153/0x6E/0x6F, denominator
            // 0x70:0x72, order in PLL1_MASHCTRL bits 2:0.
            let regs = &c.regs;
            assert_eq!(u64::from(apll1.n), number(regs, &[0x6c, 0x6d]), "{what}: N");
            assert_eq!(apll1.num, number(regs, &[0x153, 0x6e, 0x6f]), "{what}: NUM");
            assert_eq!(apll1.den, number(regs, &[0x70, 0x71, 0x72]), "{what}: DEN");
            assert_eq!(apll1.order as u8, regs[0x73].expect("staged") & 7, "{what}");
        }
    }
    assert!(exact > 100, "the sweep reaches APLL1 often ({exact})");
    assert_eq!(
        baw_tables.into_iter().collect::<Vec<_>>(),
        [12_800_000, 25_000_000, 26_000_000, 52_000_000],
        "every BAW table is staged"
    );
}

/// `SimpleSync`'s measurement LO, retuned again and again as its control does: outputs 0
/// to 3 from APLL2, each plan starting from the last one's state.
#[test]
fn simplesync_lo_plans_match_libusdr() {
    let mut apll2 = Apll2::cascaded();
    let mut lcg = Lcg(0x0005_5318);
    let mut frequencies = vec![
        3_500_000,
        10_000_000,
        100_000_000,
        444_000_000,
        1_250_000_000,
    ];
    frequencies.extend((0..600).map(|_| 3_500_000 + lcg.below(1_300_000_000)));
    frequencies.extend((0..200).map(|_| 1_000 * (3_500 + lcg.below(500_000))));
    let mut solved = 0;
    for hz in frequencies {
        let lo = [0, 1, 2, 3].map(|port| {
            OutputRequest::new(port, hz, OutputFormat::Lvds).with_affinity(Affinity::Apll2)
        });
        solved +=
            usize::from(assert_plans_match(&mut apll2, &lo, &format!("LO {hz} Hz")).is_some());
    }
    assert!(solved > 250, "many LOs are reachable ({solved})");
}

/// XMASS's calibration LO on output 4 from APLL2 beside the 25 MHz reference on output 6,
/// retuned as its control does; below 3.1 MHz output 4 is off and APLL2 keeps its state.
#[test]
fn xmass_calibration_plans_match_libusdr() {
    let mut apll2 = Apll2::cascaded();
    let mut lcg = Lcg(0x000a_5318);
    let mut frequencies = vec![444_000_000, 0, 3_099_999, 3_100_000, 1_000_000_000];
    frequencies.extend((0..500).map(|_| lcg.below(2_000_000_000)));
    for calibration in frequencies {
        let lo = if f64::from(calibration) < 3.1e6 {
            0
        } else {
            calibration
        };
        let format = if lo == 0 {
            OutputFormat::Off
        } else {
            OutputFormat::Lvds
        };
        let off = |port| OutputRequest::new(port, 0, OutputFormat::Off);
        let requests = [
            off(0),
            off(1),
            off(2),
            off(3),
            OutputRequest::new(4, lo, format).with_affinity(Affinity::Apll2),
            off(5),
            OutputRequest::new(6, 25_000_000, OutputFormat::LvcmosPN)
                .with_affinity(Affinity::Apll1),
            off(7).with_affinity(Affinity::Apll1),
        ];
        assert_plans_match(
            &mut apll2,
            &requests,
            &format!("calibration {calibration} Hz"),
        );
    }
}

/// Mixed requests: one to four distinct outputs, any format and affinity, some inverted,
/// some exact APLL1 divisions; refusals (LVCMOS low, inversion off APLL1, conflicting
/// pairs, out of range) must agree too.
#[test]
fn mixed_request_plans_match_libusdr() {
    let mut lcg = Lcg(0x00f0_5318);
    let mut outcomes = [0_usize; 2];
    let (mut muxes, mut formats, mut seen) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
    // Designed: 857142857 Hz needs post-divider 7 (VCO 5999999999 Hz), 1 GHz one of 2, 3
    // or 6 from the same VCO, so one output takes post-divider 2 (`APLL2_P2`).
    let lo =
        |port, hz| OutputRequest::new(port, hz, OutputFormat::Lvds).with_affinity(Affinity::Apll2);
    let mut cases = vec![
        vec![lo(4, 857_142_857), lo(5, 1_000_000_000)],
        vec![
            lo(5, 1_000_000_000),
            lo(4, 857_142_857),
            lo(1, 1_000_000_000),
        ],
    ];
    for _ in 0..1500 {
        let count = 1 + lcg.below(4) as usize;
        let mut ports: Vec<u8> = Vec::new();
        while ports.len() < count {
            let port = u8::try_from(lcg.below(8)).expect("below 8");
            if !ports.contains(&port) {
                ports.push(port);
            }
        }
        let requests: Vec<OutputRequest> = ports
            .iter()
            .map(|&port| {
                let hz = match lcg.below(4) {
                    0 => 2_500_000_000 / (1 + lcg.below(256)),
                    1 => 3_200_000 + lcg.below(1_200_000_000),
                    2 if port == 7 => 100_000 + lcg.below(5_000_000),
                    _ => 1_000 * (3_200 + lcg.below(800_000)),
                };
                let mut request = OutputRequest::new(port, hz, FORMATS[lcg.below(16) as usize])
                    .with_affinity(match lcg.below(3) {
                        0 => Affinity::Any,
                        1 => Affinity::Apll1,
                        _ => Affinity::Apll2,
                    });
                request.inverted = lcg.below(8) == 0;
                request
            })
            .collect();
        cases.push(requests);
    }
    for (case, requests) in cases.iter().enumerate() {
        let mut apll2 = Apll2::cascaded();
        let plan = assert_plans_match(&mut apll2, requests, &format!("case {case}: {requests:?}"));
        outcomes[usize::from(plan.is_some())] += 1;
        let Some(plan) = plan else {
            continue;
        };
        if let Apll2Setting::On { pd1, pd2, .. } = plan.apll2 {
            seen.insert(if pd1 == pd2 {
                "one post-divider"
            } else {
                "two post-dividers"
            });
        }
        for output in &plan.outputs {
            let Some(route) = output.route else {
                continue;
            };
            muxes.insert(route.mux as u8);
            formats.insert(output.request.format as u8);
            match output.request.port {
                1 | 3 => _ = seen.insert("format-only output"),
                7 if route.divider > 256 => _ = seen.insert("output 7 second stage"),
                _ => {}
            }
        }
    }
    assert!(
        outcomes.iter().all(|&n| n > 100),
        "both outcomes occur ({outcomes:?})"
    );
    assert_eq!(muxes.len(), 4, "every mux code is staged: {muxes:?}");
    assert_eq!(formats.len(), 16, "every format is staged: {formats:?}");
    assert_eq!(seen.len(), 4, "{seen:?}");
}

/// A plan refused before routing leaves no trace in either; the refusals are the C's.
#[test]
fn malformed_requests_are_refused_like_libusdr() {
    let cases: [(&str, Vec<OutputRequest>, ClockPlanError); 4] = [
        (
            "output 8",
            vec![OutputRequest::new(8, 25_000_000, OutputFormat::Lvds)],
            ClockPlanError::BadRequest,
        ),
        (
            "LVCMOS on output 2",
            vec![OutputRequest::new(2, 25_000_000, OutputFormat::LvcmosPN)],
            ClockPlanError::LvcmosOnLowOutput,
        ),
        (
            "outputs 0 and 1 differ",
            vec![
                OutputRequest::new(0, 25_000_000, OutputFormat::Lvds),
                OutputRequest::new(1, 50_000_000, OutputFormat::Lvds),
            ],
            ClockPlanError::ConflictingRequests,
        ),
        (
            "inverted from APLL2",
            vec![{
                let mut request = OutputRequest::new(4, 444_000_000, OutputFormat::Lvds);
                request.inverted = true;
                request
            }],
            ClockPlanError::InversionNeedsApll1,
        ),
    ];
    for (what, requests, error) in cases {
        let mut apll2 = Apll2::cascaded();
        assert!(assert_plans_match(&mut apll2, &requests, what).is_none());
        assert_eq!(Apll2::cascaded().plan(&requests), Err(error), "{what}");
    }
}

/// Output 7's two divider stages over its whole range, against the registers libusdr's
/// `lmk05318_set_out_div` stages, and the other outputs' range check.
#[test]
fn output_7_divider_stages_match_libusdr() {
    let mut lcg = Lcg(0x0007_5318);
    let mut dividers: Vec<u64> = (1..=600).collect();
    dividers.extend([1 << 24, (1 << 24) + 1, 256 << 24, (256 << 24) + 1, 1 << 32]);
    dividers.extend((0..2000).map(|_| u64::from(lcg.next()) + 1));
    dividers.extend(
        (0..500).map(|_| u64::from(6 + lcg.below(251)) * u64::from(1 + lcg.below(1 << 24))),
    );
    for divider in dividers {
        let (c_res, regs) = oracle::out_div(7, divider);
        assert_eq!(
            c_res == 0,
            plan::divider_fits(7, divider),
            "divider {divider}"
        );
        if c_res != 0 {
            continue;
        }
        let (first, second) = out7_stages(divider);
        let c_first = u64::from(regs[0x43].expect("OUTDIV_7 staged")) + 1;
        let c_second = number(&regs, &[0x40, 0x41, 0x42]) + 1;
        assert_eq!(
            (
                (u64::from(first) - 1) % 256 + 1,
                (second - 1) % (1 << 24) + 1
            ),
            (c_first, c_second),
            "divider {divider}"
        );
    }
    for port in 0..7 {
        for divider in [0, 1, 255, 256, 257] {
            let (c_res, _) = oracle::out_div(port, divider);
            let fits = plan::divider_fits(u8::try_from(port).unwrap(), divider);
            assert_eq!(c_res == 0, fits, "output {port} divider {divider}");
        }
    }
}

/// Characterization, not specification: the constant GCC's `-Ofast` puts in place of the
/// division by `VCO_APLL1` in `lmk05318_calc_vco2_div` (`.LC70`, 0x3DFB7CDFD9D7BDBB), read
/// from `gcc -S` output of GCC 16.2.1 (Fedora, x86-64) on 2026-10-12. It is `1 / 2.5e9`
/// rounded to `f64`; re-read it if the oracle's compiler changes. The solver results it
/// feeds do not depend on it (see `plan.rs`), so no parity test can catch it.
#[test]
fn the_vco1_reciprocal_is_gccs_constant() {
    assert_eq!(plan::VCO1_RECIPROCAL.to_bits(), 0x3dfb_7cdf_d9d7_bdbb);
}
