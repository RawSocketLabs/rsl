//! `rsl_usdr::dsp::{trig, sincos, nco}` against libusdr's compiled `xdsp/trig.c`,
//! `sincos_functions.c` (generic kernels) and `nco.c`: every fixed-point phase, tones and
//! chirps over several calls so phase carries, and the NCO when its SSSE3 kernel is built.
//!
//! The 32-bit phase and chirp-step sums are signed `int32_t` in C, undefined on overflow
//! (Rust wraps), so every sequence here sweeps within the `i32` range: from one end toward
//! the other, which still covers every quadrant. Downward NCO sweeps shift a negative
//! step left (`delta << 3`), undefined in ISO C but defined by GCC, which the oracle builds
//! with. A gain of `i16::MIN` can scale to 32768, undefined as `int16_t`, so it is
//! excluded (Rust saturates). The float kernels call the
//! platform's `sincosf` and Rust's `f32::sin_cos` its `sinf`/`cosf`; on glibc these share
//! one implementation, which is what makes bitwise comparison meaningful.

use std::num::NonZeroI32;

use rsl_usdr::dsp::nco::Nco;
use rsl_usdr::dsp::sincos::{self, Amplitude, Chirp, Tone};
use rsl_usdr::dsp::trig;
use rsl_usdr_oracle::nco;

/// Extreme 16-bit values.
const EXTREME_I16: [i16; 5] = [i16::MIN, i16::MAX, 0, -1, 1];

/// Gains tried besides random ones: full scale, unit, zero and negative.
const GAINS: [i16; 6] = [i16::MAX, -i16::MAX, 1, -1, 0, 12_345];

/// Pairs per call in a sequence, including empty and odd calls: 405 in all.
const CALLS: [usize; 5] = [0, 1, 7, 64, 333];

/// Deterministic xorshift64.
struct Rng(u64);

impl Rng {
    /// The next 64 random bits.
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A random value in `0..bound`.
    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }

    /// A random `i32` in `-bound..=bound`.
    fn within(&mut self, bound: i32) -> i32 {
        let span = u64::try_from(bound).expect("non-negative") * 2 + 1;
        i32::try_from(i64::try_from(self.below(span)).expect("small") - i64::from(bound))
            .expect("within the bound")
    }

    /// `len` random samples, a quarter of them extremes.
    fn samples(&mut self, len: usize) -> Vec<i16> {
        (0..len)
            .map(|_| {
                if self.below(4) == 0 {
                    EXTREME_I16[usize::try_from(self.below(5)).expect("small")]
                } else {
                    i16::from_le_bytes(self.next().to_le_bytes()[..2].try_into().expect("two"))
                }
            })
            .collect()
    }

    /// A gain other than `i16::MIN`.
    fn gain(&mut self) -> i16 {
        let index = usize::try_from(self.below(8)).expect("small");
        GAINS
            .get(index)
            .copied()
            .unwrap_or_else(|| i16::try_from(self.within(i32::from(i16::MAX))).expect("within i16"))
    }
}

/// Every inversion combination.
const INVERSIONS: [(bool, bool); 4] = [(false, false), (true, false), (false, true), (true, true)];

/// Starting phases and steps whose `total` steps stay within `i32`: upward from near
/// `i32::MIN` and downward from near `i32::MAX`, with the largest such step, a random one
/// and zero.
fn sweeps(rng: &mut Rng, total: usize) -> Vec<(i32, i32)> {
    let total = i64::try_from(total).expect("small");
    let widest = i32::try_from((i64::from(u32::MAX) - (1 << 20)) / total).expect("fits");
    let mut sweeps = Vec::new();
    for delta in [widest, rng.within(widest).abs(), 0] {
        let offset = rng.within(1 << 19).abs();
        sweeps.push((i32::MIN + offset, delta));
        sweeps.push((i32::MAX - offset, -delta));
    }
    sweeps
}

#[test]
fn every_fixed_point_phase_matches_isincos() {
    for phase in i16::MIN..=i16::MAX {
        assert_eq!(trig::sin_cos(phase), nco::isincos(phase), "phase {phase}");
    }
}

#[test]
fn sine_tables_match_for_every_phase_and_edge_length() {
    let all: Vec<i16> = (i16::MIN..=i16::MAX).collect();
    for phases in [&all[..], &[], &[i16::MIN], &[-32767, 0, 32767], &all[..7]] {
        let (c_sin, c_cos) = nco::sin_cos_i16(phases);
        let mut sin = vec![0; phases.len()];
        let mut cos = vec![0; phases.len()];
        sincos::sin_cos_i16(phases, &mut sin, &mut cos);
        assert_eq!((sin, cos), (c_sin, c_cos), "{} phases", phases.len());
    }
}

#[test]
fn tones_match_and_carry_their_phase_across_calls() {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let total = CALLS.iter().sum();
    for (start, delta) in sweeps(&mut rng, total) {
        for invert in INVERSIONS {
            let amplitude = Amplitude {
                gain: rng.gain(),
                invert_sin: invert.0,
                invert_cos: invert.1,
            };
            let mut rust = Tone::new(start, delta, amplitude);
            let mut c_phase = start;
            for iters in CALLS {
                let c = nco::tone(&mut c_phase, delta, amplitude.gain, invert, iters);
                let mut out = vec![0; 2 * iters];
                rust.fill(&mut out);
                let case = format!("tone {start}+{delta}, {amplitude:?}, call of {iters}");
                assert_eq!(out, c, "{case}");
                assert_eq!(rust.phase(), c_phase, "{case}: phase");
            }
        }
    }
}

#[test]
fn chirps_match_and_carry_phase_and_step_across_calls() {
    let mut rng = Rng(0xd1b5_4a32_d192_ed03);
    for case in 0..64 {
        // Steps within ±2^22 and a start within ±2^28 keep 405 phase sums and every step
        // sum within i32.
        let mut bounds = [rng.within(1 << 22), rng.within(1 << 22)];
        if case % 4 != 0 {
            bounds.sort_unstable();
        }
        let steps = match case % 8 {
            0 => 1,
            1 => -1,
            2 => i32::MAX,
            _ => rng.within(500),
        };
        let Some(steps) = NonZeroI32::new(steps) else {
            continue;
        };
        let start = rng.within(1 << 28);
        let start_delta = rng.within(1 << 22);
        let invert = INVERSIONS[case % 4];
        let amplitude = Amplitude {
            gain: rng.gain(),
            invert_sin: invert.0,
            invert_cos: invert.1,
        };
        let mut rust = Chirp::new(start, start_delta, bounds[0], bounds[1], steps, amplitude);
        let (mut c_phase, mut c_delta) = (start, start_delta);
        for iters in CALLS {
            let c = nco::chirp(
                &mut c_phase,
                &mut c_delta,
                bounds,
                steps.get(),
                amplitude.gain,
                invert,
                iters,
            );
            let mut out = vec![0; 2 * iters];
            rust.fill(&mut out);
            let what = format!("chirp {case}: {bounds:?} in {steps}, call of {iters}");
            assert_eq!(out, c, "{what}");
            assert_eq!(
                (rust.phase(), rust.delta()),
                (c_phase, c_delta),
                "{what}: state"
            );
        }
    }
}

#[test]
fn a_full_range_chirp_step_truncates_to_32_bits_like_libusdr() {
    // (i32::MAX - i32::MIN) / ±1 does not fit int32_t; GCC keeps the low 32 bits, ∓1, so
    // the step creeps from 0 without overflowing.
    let amplitude = Amplitude {
        gain: i16::MAX,
        invert_sin: false,
        invert_cos: false,
    };
    for steps in [1, -1] {
        let mut rust = Chirp::new(
            0,
            0,
            i32::MIN,
            i32::MAX,
            NonZeroI32::new(steps).expect("nonzero"),
            amplitude,
        );
        let (mut c_phase, mut c_delta) = (0, 0);
        let c = nco::chirp(
            &mut c_phase,
            &mut c_delta,
            [i32::MIN, i32::MAX],
            steps,
            i16::MAX,
            (false, false),
            5,
        );
        let mut out = [0; 10];
        rust.fill(&mut out);
        assert_eq!(out[..], c[..], "steps {steps}");
        assert_eq!(
            (rust.phase(), rust.delta()),
            (c_phase, c_delta),
            "steps {steps}"
        );
    }
}

#[test]
fn libusdrs_nco_shift_is_the_ssse3_kernel_or_the_stub() {
    let (out, phase) = nco::nco(1, 1, &[1000; 16]);
    let stub = phase == 0 && out.iter().all(|&value| value == 0);
    let kernel = phase == 9 && out.iter().any(|&value| value != 0);
    assert!(
        stub || kernel,
        "nco_shift returned {phase} and wrote {out:?}"
    );
}

#[test]
#[cfg_attr(
    not(any(target_arch = "x86", target_arch = "x86_64")),
    ignore = "build.rs compiles nco.c's SSSE3 kernel only on x86"
)]
fn nco_shifts_like_libusdrs_ssse3_kernel() {
    assert!(
        nco::nco_compiled(),
        "nco_shift is the stub: the oracle should build nco.c with -mssse3 on x86"
    );
    let mut rng = Rng(0x8c3f_8f8e_4f2a_9b61);
    // Multiples of eight: the kernel's block size.
    let calls = [0, 8, 16, 64, 1024];
    let total = calls.iter().sum();
    let mut sweeps = sweeps(&mut rng, total);
    // Through libusdr's quarter-turn fold glitch at 2^30.
    sweeps.push(((1 << 30) - 300, 1));
    sweeps.push(((1 << 30) + (1 << 15) + 300, -1));
    for (start, delta) in sweeps {
        let mut rust = Nco::new(start, delta);
        let mut c_phase = start;
        for samples in calls {
            let input = rng.samples(2 * samples);
            let (c, next) = nco::nco(c_phase, delta, &input);
            c_phase = next;
            let mut out = vec![0; input.len()];
            rust.shift(&input, &mut out);
            let case = format!("nco {start}+{delta}, call of {samples}");
            assert_eq!(out, c, "{case}");
            assert_eq!(rust.phase(), c_phase, "{case}: phase");
        }
    }

    // A partial block: libusdr shifts and advances by the whole block, Rust exactly.
    let input = rng.samples(10);
    let (c, next) = nco::nco(7, 1000, &input);
    let mut rust = Nco::new(7, 1000);
    let mut out = vec![0; input.len()];
    rust.shift(&input, &mut out);
    assert_eq!(out, c, "the samples given shift alike");
    assert_eq!((next, rust.phase()), (7 + 8 * 1000, 7 + 5 * 1000));
}
