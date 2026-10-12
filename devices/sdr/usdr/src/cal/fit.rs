//! Fitting a sinusoid to captured IQ samples (`cal/cal_filt.c`).
//!
//! Samples are [`Complex32`]: `re` is libusdr's I and `im` its Q. A generated tone has
//! `I = amp * sin(phase)` and `Q = amp * cos(phase)`, as `cal_generate_iq` makes it.
//!
//! The C mixes single and double precision: `sqrt` and the tone's phase are computed in
//! double and stored to float, while sums accumulate in float. The functions here repeat
//! each promotion, so results match libusdr's bit for bit when C is built without fused
//! multiply-add contraction and both use the same host libm (Rust's `sin_cos` calls `sinf`
//! and `cosf` where C calls `sincosf`). Empty inputs divide by zero and yield NaN, as in
//! the C.

use core::f64::consts::PI;

use num_complex::Complex32;

use super::search::{Axis, Limits, Objective2d, Search, Stage, best_2d, golden_min};

/// Phase steps over one turn in the searches.
const PHASE_STEPS: i32 = 1024;

/// A fitted tone.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fit {
    /// The fit's error: the summed distance between samples and tone, as found by the
    /// integer search (so quantized to its scale).
    pub error: f32,
    /// The tone's starting phase, in radians.
    pub phase: f32,
    /// The tone's period, in samples.
    pub period: f32,
}

/// Unpacks 12-bit I (bits 11:0) and Q (bits 23:12) sample words into full-scale ±1.0
/// samples (`convert_dump12_to_cfloat`). Converts `min(words, out)` samples.
pub fn from_dump12(words: &[u32], out: &mut [Complex32]) {
    for (word, sample) in words.iter().zip(out) {
        *sample = Complex32::new(dump12(word & 0xfff), dump12((word >> 12) & 0xfff));
    }
}

/// One 12-bit field as a signed fraction of full scale.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the field is 12 bits; C stores the double quotient to float"
)]
fn dump12(field: u32) -> f32 {
    // C: (int16_t)(uint16_t)(field << 4), then / 32768.0 in double.
    let signed = i16::from_ne_bytes(((field << 4) as u16).to_ne_bytes());
    (f64::from(signed) / 32768.0) as f32
}

/// C's `sqrt` on a float expression: computed in double, stored to float.
#[expect(
    clippy::cast_possible_truncation,
    reason = "C stores the double square root to float"
)]
fn sqrt(value: f32) -> f32 {
    f64::from(value).sqrt() as f32
}

/// `count` as the float C converts an `unsigned` divisor to.
#[expect(
    clippy::cast_precision_loss,
    reason = "C converts the unsigned count to float"
)]
fn count(samples: &[Complex32]) -> f32 {
    samples.len() as f32
}

/// Subtracts the mean from every sample and returns that mean (`cal_remove_dc`).
pub fn remove_dc(samples: &mut [Complex32]) -> Complex32 {
    let mut sum = Complex32::new(0.0, 0.0);
    for sample in samples.iter() {
        sum.re += sample.re;
        sum.im += sample.im;
    }
    let n = count(samples);
    let mean = Complex32::new(sum.re / n, sum.im / n);
    for sample in samples.iter_mut() {
        sample.re -= mean.re;
        sample.im -= mean.im;
    }
    mean
}

/// The mean and standard deviation of the samples' magnitudes (`calculate_mean_s_std`).
#[must_use]
pub fn magnitude_mean_std(samples: &[Complex32]) -> (f32, f32) {
    let magnitude = |s: &Complex32| sqrt(s.re * s.re + s.im * s.im);
    let n = count(samples);
    let mut mean = 0.0_f32;
    for sample in samples {
        mean += magnitude(sample);
    }
    mean /= n;
    let mut variance = 0.0_f32;
    for sample in samples {
        let v = magnitude(sample);
        variance += (v - mean) * (v - mean);
    }
    variance /= n;
    (mean, sqrt(variance))
}

/// Fills `out` with a tone of amplitude `amp`, `period` samples long, starting at phase
/// `phase` radians (`cal_generate_iq`).
pub fn generate(out: &mut [Complex32], amp: f32, period: f32, phase: f32) {
    for (i, sample) in (0_u32..).zip(out.iter_mut()) {
        let (sin, cos) = tone_phase(i, period, phase).sin_cos();
        *sample = Complex32::new(sin * amp, cos * amp);
    }
}

/// Sample `i`'s phase: `ph + 2 * M_PI * i / period` in double, stored to float.
#[expect(
    clippy::cast_possible_truncation,
    reason = "C stores the double phase to float"
)]
fn tone_phase(i: u32, period: f32, phase: f32) -> f32 {
    (f64::from(phase) + 2.0 * PI * f64::from(i) / f64::from(period)) as f32
}

/// The summed distance between the samples and a tone of amplitude `amp`, `period`
/// samples long, starting at `phase` radians (`_cal_mse_fit`): the cost the fits minimise.
#[expect(
    clippy::cast_possible_truncation,
    reason = "C accumulates each double root into a float"
)]
#[must_use]
pub fn fit_error(samples: &[Complex32], amp: f32, period: f32, phase: f32) -> f32 {
    let mut error = 0.0_f32;
    for (i, sample) in (0_u32..).zip(samples) {
        let (sin, cos) = tone_phase(i, period, phase).sin_cos();
        let di = sample.re - sin * amp;
        let dq = sample.im - cos * amp;
        error = (f64::from(error) + f64::from(di * di + dq * dq).sqrt()) as f32;
    }
    error
}

/// The phase step's size in radians, as the float the C stores (`2 * M_PI / ph_steps`).
#[expect(
    clippy::cast_possible_truncation,
    reason = "C stores the double step to float"
)]
fn phase_scale() -> f32 {
    (2.0 * PI / f64::from(PHASE_STEPS)) as f32
}

/// `value * scale` as C's `int * float`.
#[expect(
    clippy::cast_precision_loss,
    reason = "C converts the integer step to float"
)]
fn scaled(value: i32, scale: f32) -> f32 {
    value as f32 * scale
}

/// `error * scale`, truncated to the search's integer cost. Rust saturates where C's
/// out-of-range conversion is undefined (`INT_MIN` on x86), so results match libusdr only
/// while the summed error times the scale stays below `i32::MAX`.
#[expect(
    clippy::cast_possible_truncation,
    reason = "C truncates the float cost to int"
)]
fn cost(error: f32, scale: f32) -> i32 {
    (error * scale) as i32
}

/// The phase that best fits a tone of known amplitude and period, by golden-section search
/// over 1024 phase steps (`cal_find_best_fit`). [`Fit::period`] is `period`.
#[must_use]
pub fn best_phase(samples: &[Complex32], amp: f32, period: f32) -> Fit {
    const SCALE: f32 = 1000.0;
    let step = phase_scale();
    let found = golden_min::<core::convert::Infallible>(0, PHASE_STEPS - 1, |value| {
        Ok(cost(
            fit_error(samples, amp, period, scaled(value, step)),
            SCALE,
        ))
    });
    let Ok(found) = found;
    Fit {
        error: scaled(found.cost, 1.0) / SCALE,
        phase: scaled(found.x, step),
        period,
    }
}

/// The phase and period that best fit a tone of known amplitude, by two rounds of
/// coordinate-wise golden-section search over 1024 phase steps and periods of 8 to 384
/// samples in hundredths (`cal_find_best_fit_ph_per`). Libusdr's windowing quirks apply
/// (see [`best_2d`]).
#[must_use]
pub fn best_phase_and_period(samples: &[Complex32], amp: f32) -> Fit {
    const SCALE: f32 = 10000.0;
    const PERIOD_SCALE: f32 = 100.0;
    let stage = Stage {
        search: Search::Golden,
        limits: [
            Limits {
                min: 0,
                max: PHASE_STEPS - 1,
            },
            Limits {
                min: 800,
                max: 38_400,
            },
        ],
        extra_step: 0,
    };
    let mut objective = ToneObjective {
        samples,
        amp,
        phase: 0.0,
        period: 0.0,
        phase_scale: phase_scale(),
        period_scale: PERIOD_SCALE,
        scale: SCALE,
    };
    let Ok(best) = best_2d(&[stage, stage], 0, &mut objective);
    Fit {
        error: scaled(best.cost, 1.0) / SCALE,
        phase: scaled(best.x, objective.phase_scale),
        period: scaled(best.y, 1.0) / PERIOD_SCALE,
    }
}

/// The tone fit as a two-dimensional objective: X is the phase step, Y the period in
/// hundredths (`_evaluate_mse_fit_pp_fn`).
struct ToneObjective<'a> {
    /// The captured samples.
    samples: &'a [Complex32],
    /// The tone's amplitude.
    amp: f32,
    /// The current phase, in radians.
    phase: f32,
    /// The current period, in samples.
    period: f32,
    /// Radians per phase step.
    phase_scale: f32,
    /// Period steps per sample.
    period_scale: f32,
    /// Cost per unit of fit error.
    scale: f32,
}

impl Objective2d for ToneObjective<'_> {
    type Error = core::convert::Infallible;

    fn set(&mut self, _stage: usize, axis: Axis, value: i32) -> Result<(), Self::Error> {
        match axis {
            Axis::X => self.phase = scaled(value, self.phase_scale),
            Axis::Y => self.period = scaled(value, 1.0) / self.period_scale,
        }
        Ok(())
    }

    fn evaluate(&mut self, stage: usize, axis: Axis, value: i32) -> Result<i32, Self::Error> {
        self.set(stage, axis, value)?;
        Ok(cost(
            fit_error(self.samples, self.amp, self.period, self.phase),
            self.scale,
        ))
    }
}

/// The largest per-component distance between the samples and a tone
/// (`cal_max_amp_error`).
#[must_use]
pub fn max_component_error(samples: &[Complex32], amp: f32, period: f32, phase: f32) -> f32 {
    let mut error = 0.0_f32;
    for (i, sample) in (0_u32..).zip(samples) {
        let (sin, cos) = tone_phase(i, period, phase).sin_cos();
        let di = (sample.re - sin * amp).abs();
        let dq = (sample.im - cos * amp).abs();
        if error < di {
            error = di;
        }
        if error < dq {
            error = dq;
        }
    }
    error
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generated_tone_is_fitted_back_to_its_phase() {
        let mut tone = [Complex32::new(0.0, 0.0); 256];
        generate(&mut tone, 0.5, 32.0, 1.0);
        let fit = best_phase(&tone, 0.5, 32.0);
        assert!(
            (fit.phase - 1.0).abs() < 2.0 * phase_scale(),
            "within two phase steps"
        );
    }

    #[test]
    fn removing_dc_returns_the_mean_and_centres_the_samples() {
        let mut samples = [Complex32::new(1.0, -1.0), Complex32::new(3.0, 1.0)];
        assert_eq!(remove_dc(&mut samples), Complex32::new(2.0, 0.0));
        assert_eq!(samples[0], Complex32::new(-1.0, -1.0));
    }

    #[test]
    fn dump12_fields_are_sign_extended_to_full_scale() {
        let mut out = [Complex32::new(0.0, 0.0); 1];
        // I = 0x7ff (most positive), Q = 0x800 (most negative).
        from_dump12(&[0x800_7ff], &mut out);
        assert_eq!(out[0], Complex32::new(2047.0 / 2048.0, -1.0));
    }

    #[test]
    fn a_constant_magnitude_has_zero_spread() {
        let samples = [Complex32::new(0.6, 0.8); 4];
        let (mean, std) = magnitude_mean_std(&samples);
        assert!((mean - 1.0).abs() < 1e-6);
        assert!(std.abs() < 1e-6);
    }
}
