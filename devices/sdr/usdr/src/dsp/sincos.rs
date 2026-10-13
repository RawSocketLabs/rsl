//! Floating-point sine/cosine tables, tones and chirps as 16-bit integers: the port of
//! libusdr's `xdsp/sincos_functions.c` and its generic kernels
//! (`templates/wvlt_sincos_i16*_generic.t`), with the conversions its callers use
//! (`WVLT_CONVPHASE_F32_I32`, `DBFS_TO_AMPLITUDE`), and the floating-point tone of
//! libusdr's `usdr_dm_create` tool ([`ToneF32`]).
//!
//! # Test signals as `usdr_dm_create` makes them
//!
//! The tool's `ci16` generators build a [`Tone`] or [`Chirp`] from turns, Hz and dBFS,
//! inverting the sine, at a full scale of 32760, and carry it from packet to packet:
//!
//! ```
//! use num_complex::Complex;
//! use rsl_usdr::dsp::sincos::{Amplitude, Tone, gain_from_dbfs, phase_from_turns};
//!
//! let amplitude = Amplitude {
//!     gain: gain_from_dbfs(-6.0, 32760).expect("within int16_t"),
//!     invert_sin: true,
//!     invert_cos: false,
//! };
//! let start = phase_from_turns(0.0).expect("within the turn");
//! let step = phase_from_turns(0.02).expect("within the turn"); // 0.02 × the sample rate
//! let mut tone = Tone::new(start, step, amplitude);
//! let mut packet = vec![Complex::new(0, 0); 4096];
//! tone.fill_complex(&mut packet); // then `Device::send`, and the next packet
//! ```
//!
//! The tool times its first packet one packet's samples into the stream and each later
//! one right after the last (`do_transmit`); the oracle's `generator_parity` test sends
//! that way through both drivers.
//!
//! Phases convert to radians in double precision and are stored to float, as in the C;
//! the sine and cosine are single-precision, taken from the platform's math library (the
//! C's `sincosf`, Rust's `f32::sin_cos`); and results scale in float and truncate toward
//! zero. With the same `libm`, as on glibc where the oracle's `nco_parity` test runs,
//! results match libusdr's bit for bit. Unlike the rest of [`super`], this needs `std`.
//!
//! Where libusdr is undefined, this is defined instead:
//!
//! - A gain of `i16::MIN` scales a sine or cosine of exactly -1 (or +1 inverted) to
//!   32768, which does not fit `int16_t`; results saturate here.
//! - libusdr's carried phase and chirp step are signed 32-bit sums, undefined on overflow;
//!   here they wrap.
//!
//! libusdr's SSSE3 and NEON variants, which approximate the sine differently, are not
//! ported.

use core::f64::consts::PI;
use core::num::{NonZeroI32, NonZeroU32};

use num_complex::Complex;

/// Radians per unit of a 16-bit phase: π/2 at 32767 (`WVLT_SINCOS_I16_PHSCALE`).
const I16_PHASE_SCALE: f64 = PI / (2.0 * 32767.0);
/// Radians per unit of a 32-bit phase: π at `i32::MAX` (`WVLT_SINCOS_I32_PHSCALE`).
const I32_PHASE_SCALE: f64 = PI / 2_147_483_647.0;
/// Full scale of [`sin_cos_i16`]'s outputs (`WVLT_SINCOS_I16_SCALE`).
const I16_SCALE: f32 = 32767.0;
/// `(float)INT32_MIN`, which `WVLT_CONVPHASE_F32_I32` scales turns by.
const I32_MIN_F: f64 = -2_147_483_648.0;
/// dB per doubling of amplitude, as `DBFS_TO_AMPLITUDE` writes it.
#[expect(
    clippy::excessive_precision,
    reason = "libusdr's literal as written; it rounds to the same float"
)]
const DB_PER_DOUBLING: f32 = 6.020_599_913;

/// A phase in turns as a 32-bit phase (`WVLT_CONVPHASE_F32_I32`): 0 to just under ½ turn
/// map to 0 to `i32::MAX`, ½ to 1 turn to `i32::MIN` to 0, and negative turns above -½ to
/// negative phases. Each is scaled in double and truncated toward zero, as in the C.
///
/// `None` where the C conversion is undefined: NaN, -½ turn or less, or just past one
/// turn (beyond `1 + 2⁻³²`).
#[must_use]
pub fn phase_from_turns(turns: f64) -> Option<i32> {
    if turns >= 0.5 {
        let part = c_int32((turns - 0.5) * 2.0 * I32_MIN_F)?;
        i32::MIN.checked_sub(part)
    } else {
        c_int32(turns * 2.0 * I32_MIN_F)?.checked_neg()
    }
}

/// `full_scale` attenuated by `dbfs` decibels (`DBFS_TO_AMPLITUDE`): `full_scale ×
/// 2^(dbfs / 6.020599913)`, the exponent divided in float and the rest in double, truncated
/// toward zero. libusdr's `usdr_dm_create` uses a full scale of 32760.
///
/// `None` where the C's conversion to `int16_t` is undefined: NaN, or a gain past the
/// 16-bit range (a few thousandths of a dB above 0 dBFS at the tool's full scale).
#[must_use]
pub fn gain_from_dbfs(dbfs: f32, full_scale: i16) -> Option<i16> {
    let gain = f64::from(full_scale) * 2_f64.powf(f64::from(dbfs / DB_PER_DOUBLING));
    c_int16(gain)
}

/// Writes the sine and cosine of each 16-bit `phase` (±32767 is ±π/2) to `sin` and `cos`,
/// at full scale ±32767 (`wvlt_sincos_i16`). Converts as many phases as all three buffers
/// hold.
pub fn sin_cos_i16(phase: &[i16], sin: &mut [i16], cos: &mut [i16]) {
    for ((&phase, sin), cos) in phase.iter().zip(sin).zip(cos) {
        let (s, c) = radians(I16_PHASE_SCALE, phase.into()).sin_cos();
        *sin = truncate(s * I16_SCALE);
        *cos = truncate(c * I16_SCALE);
    }
}

/// How a [`Tone`] or [`Chirp`] scales its sine and cosine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Amplitude {
    /// The peak output value.
    pub gain: i16,
    /// Negate the sine.
    pub invert_sin: bool,
    /// Negate the cosine.
    pub invert_cos: bool,
}

impl Amplitude {
    /// The `(sin, cos)` pair for `phase` (a full turn over the `i32` range): each value is
    /// `sin * gain * ±1` in float, truncated toward zero.
    fn pair(self, phase: i32) -> (i16, i16) {
        let (s, c) = radians(I32_PHASE_SCALE, phase.into()).sin_cos();
        let gain = f32::from(self.gain);
        (
            truncate(s * gain * sign(self.invert_sin)),
            truncate(c * gain * sign(self.invert_cos)),
        )
    }
}

/// A constant-frequency tone: interleaved `(sin, cos)` pairs whose phase advances by a
/// fixed step, carried across calls (`wvlt_sincos_i16_interleaved_ctrl`).
///
/// Phases are fractions of a turn over the `i32` range: `i32::MIN` is -π.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tone {
    /// The next pair's phase.
    phase: i32,
    /// The phase step per pair.
    delta: i32,
    /// Output scaling.
    amplitude: Amplitude,
}

impl Tone {
    /// A tone starting at `phase`, advancing `delta` per pair.
    #[must_use]
    pub const fn new(phase: i32, delta: i32, amplitude: Amplitude) -> Self {
        Self {
            phase,
            delta,
            amplitude,
        }
    }

    /// The next pair's phase: libusdr's updated `*start_phase`.
    #[must_use]
    pub const fn phase(&self) -> i32 {
        self.phase
    }

    /// Fills `out` with `out.len() / 2` pairs, each at the current phase before it steps; a
    /// lone trailing value is untouched.
    pub fn fill(&mut self, out: &mut [i16]) {
        for pair in out.chunks_exact_mut(2) {
            (pair[0], pair[1]) = self.next_pair();
        }
    }

    /// [`Tone::fill`] into complex samples, the sine as I and the cosine as Q: the `ci16`
    /// host buffer libusdr's interleaved output is.
    pub fn fill_complex(&mut self, out: &mut [Complex<i16>]) {
        for sample in out {
            let (i, q) = self.next_pair();
            *sample = Complex::new(i, q);
        }
    }

    /// The pair at the current phase, then the step.
    fn next_pair(&mut self) -> (i16, i16) {
        let pair = self.amplitude.pair(self.phase);
        self.phase = self.phase.wrapping_add(self.delta);
        pair
    }
}

/// A linear chirp: interleaved `(sin, cos)` pairs whose phase step itself sweeps between
/// two bounds, restarting at the other bound when it passes one, carried across calls
/// (`wvlt_sincos_i16_interleaved_chirp`).
///
/// Phases and steps are fractions of a turn over the `i32` range, as for [`Tone`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chirp {
    /// The phase before the next pair's step.
    phase: i32,
    /// The next phase step.
    delta: i32,
    /// The step below which the sweep restarts at `high` (`delta_phase[0]`).
    low: i32,
    /// The step above which the sweep restarts at `low` (`delta_phase[1]`).
    high: i32,
    /// The change in step per pair: `(high - low) / steps`.
    sweep: i32,
    /// Output scaling.
    amplitude: Amplitude,
}

impl Chirp {
    /// A chirp at `phase` whose first step is `delta`, sweeping between `low` and `high`
    /// in `steps` pairs.
    ///
    /// The step's change per pair is `(high - low) / steps` in 64-bit arithmetic,
    /// truncated toward zero and then to 32 bits, as libusdr computes it (a negative
    /// `steps` sweeps downward). `steps` is nonzero: libusdr divides by it.
    #[must_use]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "C stores the 64-bit quotient to int32_t, keeping its low 32 bits"
    )]
    pub const fn new(
        phase: i32,
        delta: i32,
        low: i32,
        high: i32,
        steps: NonZeroI32,
        amplitude: Amplitude,
    ) -> Self {
        let sweep = ((high as i64 - low as i64) / steps.get() as i64) as i32;
        Self {
            phase,
            delta,
            low,
            high,
            sweep,
            amplitude,
        }
    }

    /// A chirp at `start` turns sweeping between `low_hz` and `high_hz` at `sample_rate` in
    /// `steps` pairs, as libusdr's `usdr_dm_create` builds one: each bound's step is its
    /// frequency over the sample rate in turns ([`phase_from_turns`]), and the first step is
    /// the lower bound's for an upward sweep (`steps` positive), the upper's for a downward.
    ///
    /// `None` unless `low_hz` is below `high_hz` (the tool refuses others), or where a
    /// phase conversion is undefined.
    #[must_use]
    pub fn sweep(
        start: f64,
        low_hz: f64,
        high_hz: f64,
        sample_rate: NonZeroU32,
        steps: NonZeroI32,
        amplitude: Amplitude,
    ) -> Option<Self> {
        (low_hz < high_hz).then_some(())?;
        let rate = f64::from(sample_rate.get());
        let low = phase_from_turns(low_hz / rate)?;
        let high = phase_from_turns(high_hz / rate)?;
        let first = if steps.get() > 0 { low } else { high };
        Some(Self::new(
            phase_from_turns(start)?,
            first,
            low,
            high,
            steps,
            amplitude,
        ))
    }

    /// The phase the next pair steps from: libusdr's updated `*start_phase`.
    #[must_use]
    pub const fn phase(&self) -> i32 {
        self.phase
    }

    /// The next phase step: libusdr's updated `*start_delta_phase`.
    #[must_use]
    pub const fn delta(&self) -> i32 {
        self.delta
    }

    /// Fills `out` with `out.len() / 2` pairs; a lone trailing value is untouched. Each pair
    /// first steps the phase, then the step sweeps, restarting at `low` above `high` and at
    /// `high` below `low`.
    pub fn fill(&mut self, out: &mut [i16]) {
        for pair in out.chunks_exact_mut(2) {
            (pair[0], pair[1]) = self.next_pair();
        }
    }

    /// [`Chirp::fill`] into complex samples, the sine as I and the cosine as Q.
    pub fn fill_complex(&mut self, out: &mut [Complex<i16>]) {
        for sample in out {
            let (i, q) = self.next_pair();
            *sample = Complex::new(i, q);
        }
    }

    /// Steps the phase, takes its pair, then sweeps the step.
    fn next_pair(&mut self) -> (i16, i16) {
        self.phase = self.phase.wrapping_add(self.delta);
        let pair = self.amplitude.pair(self.phase);
        self.delta = self.delta.wrapping_add(self.sweep);
        if self.delta > self.high {
            self.delta = self.low;
        } else if self.delta < self.low {
            self.delta = self.high;
        }
        pair
    }
}

/// A constant-frequency tone of complex floats, its phase carried across calls: the
/// generator libusdr's `usdr_dm_create` runs for `cf32` (`freq_gen_thread_cf32`).
///
/// The phase is in turns, in double; each sample is the float sine and cosine of `2π ×
/// phase` (stored to float), scaled in float, the cosine as I and the sine as Q. After each
/// sample the phase steps, then wraps by one turn once past ±1.
///
/// The tool passes its `-g` value, meant in dBFS, as the amplitude, so its default of
/// 0 dBFS sends silence; here the amplitude is what the caller gives.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToneF32 {
    /// The next sample's phase, in turns.
    phase: f64,
    /// The phase step per sample, in turns.
    step: f64,
    /// The peak output value.
    amplitude: f32,
}

impl ToneF32 {
    /// A tone starting at `phase` turns, advancing `step` turns per sample.
    #[must_use]
    pub const fn new(phase: f64, step: f64, amplitude: f32) -> Self {
        Self {
            phase,
            step,
            amplitude,
        }
    }

    /// The next sample's phase, in turns.
    #[must_use]
    pub const fn phase(&self) -> f64 {
        self.phase
    }

    /// Fills `out`, one sample per phase step.
    pub fn fill_complex(&mut self, out: &mut [Complex<f32>]) {
        for sample in out {
            let (sin, cos) = radians(2.0 * PI, self.phase).sin_cos();
            *sample = Complex::new(self.amplitude * cos, self.amplitude * sin);
            self.phase += self.step;
            if self.phase > 1.0 {
                self.phase -= 1.0;
            } else if self.phase < -1.0 {
                self.phase += 1.0;
            }
        }
    }
}

/// `scale * phase` in double, stored to float.
#[expect(
    clippy::cast_possible_truncation,
    reason = "C stores the double phase to float"
)]
fn radians(scale: f64, phase: f64) -> f32 {
    (scale * phase) as f32
}

/// `-1.0` when `invert`, else `1.0`: libusdr's `int16_t` sign, promoted to float.
fn sign(invert: bool) -> f32 {
    if invert { -1.0 } else { 1.0 }
}

/// C's double to `int32_t` conversion, toward zero; `None` where C is undefined.
#[expect(
    clippy::cast_possible_truncation,
    reason = "in range: C truncates toward zero"
)]
fn c_int32(value: f64) -> Option<i32> {
    (value > -2_147_483_649.0 && value < 2_147_483_648.0).then_some(value as i32)
}

/// C's double to `int16_t` conversion, toward zero; `None` where C is undefined.
#[expect(
    clippy::cast_possible_truncation,
    reason = "in range: C truncates toward zero"
)]
fn c_int16(value: f64) -> Option<i16> {
    (value > -32_769.0 && value < 32_768.0).then_some(value as i16)
}

/// C's float to `int16_t` conversion, toward zero; saturating where C is undefined.
#[expect(
    clippy::cast_possible_truncation,
    reason = "C truncates toward zero; out-of-range values (undefined in C) saturate"
)]
fn truncate(value: f32) -> i16 {
    value as i16
}

#[cfg(test)]
mod tests {
    use core::f64::consts::PI;
    use core::num::{NonZeroI32, NonZeroU32};

    use num_complex::Complex;

    use super::{
        Amplitude, Chirp, DB_PER_DOUBLING, Tone, ToneF32, gain_from_dbfs, phase_from_turns,
        sin_cos_i16,
    };

    /// A plain amplitude of `gain`.
    const fn plain(gain: i16) -> Amplitude {
        Amplitude {
            gain,
            invert_sin: false,
            invert_cos: false,
        }
    }

    #[test]
    fn quarter_turns_hit_the_axes() {
        let mut sin = [0; 3];
        let mut cos = [0; 3];
        sin_cos_i16(&[-32767, 0, 32767], &mut sin, &mut cos);
        assert_eq!(sin, [-32767, 0, 32767]);
        assert_eq!(cos[1], 32767);
        assert!(cos[0].abs() <= 1 && cos[2].abs() <= 1, "cos(±π/2) is 0");
    }

    #[test]
    fn a_tone_follows_its_phase_and_inversions() {
        let amplitude = Amplitude {
            gain: 10_000,
            invert_sin: true,
            invert_cos: false,
        };
        let mut tone = Tone::new(i32::MIN, 1 << 29, amplitude);
        let mut out = [0; 17];
        tone.fill(&mut out);
        for (n, pair) in (0_i32..).zip(out.chunks_exact(2)) {
            let angle = -PI + f64::from(n) * PI / 4.0;
            let expect = (-angle.sin() * 10_000.0, angle.cos() * 10_000.0);
            assert!(
                (f64::from(pair[0]) - expect.0).abs() <= 1.0
                    && (f64::from(pair[1]) - expect.1).abs() <= 1.0,
                "pair {n}: {pair:?} vs {expect:?}"
            );
        }
        assert_eq!(out[16], 0, "the lone trailing value is untouched");
        assert_eq!(
            tone.phase(),
            i32::MIN,
            "eight eighth-turns wrap to the start"
        );
    }

    #[test]
    fn a_chirp_sweeps_its_step_and_restarts_past_the_bound() {
        let steps = NonZeroI32::new(4).expect("nonzero");
        let mut chirp = Chirp::new(0, 100, 100, 500, steps, plain(1));
        let mut out = [0; 12];
        chirp.fill(&mut out);
        // Steps 100, 200, 300, 400, 500, then 600 > 500 restarts at 100.
        assert_eq!(chirp.phase(), 100 + 200 + 300 + 400 + 500 + 100);
        assert_eq!(chirp.delta(), 200);
    }

    #[test]
    fn a_downward_chirp_restarts_at_the_upper_bound() {
        let steps = NonZeroI32::new(-2).expect("nonzero");
        let mut chirp = Chirp::new(0, 10, 0, 10, steps, plain(1));
        let mut out = [0; 6];
        chirp.fill(&mut out);
        // Steps 10, 5, 0, then -5 < 0 restarts at 10.
        assert_eq!(chirp.phase(), 15);
        assert_eq!(chirp.delta(), 10);
    }

    #[test]
    fn a_minimum_gain_at_full_scale_saturates() {
        // sin(π/2) is exactly 1.0 in f32, so -32768 * 1.0 * -1.0 is 32768: undefined in C.
        let amplitude = Amplitude {
            gain: i16::MIN,
            invert_sin: true,
            invert_cos: false,
        };
        let mut out = [0; 2];
        Tone::new(1 << 30, 0, amplitude).fill(&mut out);
        assert_eq!(out[0], i16::MAX);
    }

    #[test]
    fn turns_map_onto_the_i32_circle() {
        let cases = [
            (0.0, 0),
            (0.25, 1 << 30),
            (0.5, i32::MIN),
            (0.75, -(1 << 30)),
            (1.0, 0),
            (-0.25, -(1 << 30)),
        ];
        for (turns, phase) in cases {
            assert_eq!(phase_from_turns(turns), Some(phase), "{turns} turns");
        }
    }

    #[test]
    fn turns_outside_cs_defined_range_have_no_phase() {
        for turns in [-0.5, -0.75, 1.001, f64::NAN, f64::INFINITY] {
            assert_eq!(phase_from_turns(turns), None, "{turns} turns");
        }
        assert!(phase_from_turns(-0.499_999).is_some(), "just above -½ turn");
    }

    #[test]
    fn decibels_halve_the_gain_every_six_and_refuse_past_int16() {
        assert_eq!(gain_from_dbfs(0.0, 32760), Some(32760));
        assert_eq!(gain_from_dbfs(-DB_PER_DOUBLING, 32760), Some(16380));
        assert_eq!(gain_from_dbfs(f32::NEG_INFINITY, 32760), Some(0));
        assert_eq!(gain_from_dbfs(1.0, 32760), None, "36763 overflows int16_t");
        assert_eq!(gain_from_dbfs(f32::NAN, 32760), None);
    }

    #[test]
    fn a_swept_chirp_starts_at_the_bound_it_sweeps_from() {
        let rate = NonZeroU32::new(1_000_000).expect("nonzero");
        let up = NonZeroI32::new(7).expect("nonzero");
        let down = NonZeroI32::new(-7).expect("nonzero");
        let low = phase_from_turns(-0.1).expect("in range");
        let high = phase_from_turns(0.2).expect("in range");
        let upward =
            Chirp::sweep(0.0, -100_000.0, 200_000.0, rate, up, plain(1)).expect("valid sweep");
        let downward =
            Chirp::sweep(0.0, -100_000.0, 200_000.0, rate, down, plain(1)).expect("valid sweep");
        assert_eq!(upward.delta(), low);
        assert_eq!(downward.delta(), high);
        assert!(
            Chirp::sweep(0.0, 1.0, 1.0, rate, up, plain(1)).is_none(),
            "the lower bound must be below the upper"
        );
    }

    #[test]
    fn complex_fills_are_the_interleaved_pairs() {
        let amplitude = Amplitude {
            gain: 20_000,
            invert_sin: true,
            invert_cos: false,
        };
        let steps = NonZeroI32::new(-5).expect("nonzero");
        let mut pairs = [0; 26];
        let mut samples = [Complex::new(0, 0); 13];
        let mut tone = Tone::new(123, 456_789_012, amplitude);
        tone.clone().fill(&mut pairs);
        tone.fill_complex(&mut samples);
        let interleaved: Vec<i16> = samples.iter().flat_map(|s| [s.re, s.im]).collect();
        assert_eq!(interleaved, pairs, "tone");

        let mut chirp = Chirp::new(7, 1000, -50_000, 90_000, steps, amplitude);
        chirp.clone().fill(&mut pairs);
        chirp.fill_complex(&mut samples);
        let interleaved: Vec<i16> = samples.iter().flat_map(|s| [s.re, s.im]).collect();
        assert_eq!(interleaved, pairs, "chirp");
    }

    #[test]
    fn a_float_tone_turns_by_its_step_and_wraps_past_a_turn() {
        let mut tone = ToneF32::new(0.0, 0.25, 2.0);
        let mut out = [Complex::new(0.0, 0.0); 4];
        tone.fill_complex(&mut out);
        let expected = [(2.0, 0.0), (0.0, 2.0), (-2.0, 0.0), (0.0, -2.0)];
        for (sample, (i, q)) in out.iter().zip(expected) {
            assert!(
                (sample.re - i).abs() < 1e-6 && (sample.im - q).abs() < 1e-6,
                "{sample} vs ({i}, {q})"
            );
        }
        // Four quarter turns reach 1.0, not past it; the fifth wraps.
        assert!((tone.phase() - 1.0).abs() < 1e-12);
        tone.fill_complex(&mut out[..1]);
        assert!((tone.phase() - 0.25).abs() < 1e-12);
    }
}
