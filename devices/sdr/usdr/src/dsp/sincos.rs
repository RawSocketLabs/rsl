//! Floating-point sine/cosine tables, tones and chirps as 16-bit integers: the port of
//! libusdr's `xdsp/sincos_functions.c` and its generic kernels
//! (`templates/wvlt_sincos_i16*_generic.t`).
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
use core::num::NonZeroI32;

/// Radians per unit of a 16-bit phase: π/2 at 32767 (`WVLT_SINCOS_I16_PHSCALE`).
const I16_PHASE_SCALE: f64 = PI / (2.0 * 32767.0);
/// Radians per unit of a 32-bit phase: π at `i32::MAX` (`WVLT_SINCOS_I32_PHSCALE`).
const I32_PHASE_SCALE: f64 = PI / 2_147_483_647.0;
/// Full scale of [`sin_cos_i16`]'s outputs (`WVLT_SINCOS_I16_SCALE`).
const I16_SCALE: f32 = 32767.0;

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
    /// Writes one interleaved `(sin, cos)` pair for `phase` (a full turn over the `i32`
    /// range): each value is `sin * gain * ±1` in float, truncated toward zero.
    fn write(self, phase: i32, pair: &mut [i16]) {
        let (s, c) = radians(I32_PHASE_SCALE, phase.into()).sin_cos();
        let gain = f32::from(self.gain);
        pair[0] = truncate(s * gain * sign(self.invert_sin));
        pair[1] = truncate(c * gain * sign(self.invert_cos));
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
            self.amplitude.write(self.phase, pair);
            self.phase = self.phase.wrapping_add(self.delta);
        }
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
            self.phase = self.phase.wrapping_add(self.delta);
            self.amplitude.write(self.phase, pair);
            self.delta = self.delta.wrapping_add(self.sweep);
            if self.delta > self.high {
                self.delta = self.low;
            } else if self.delta < self.low {
                self.delta = self.high;
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
    use core::num::NonZeroI32;

    use super::{Amplitude, Chirp, Tone, sin_cos_i16};

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
}
