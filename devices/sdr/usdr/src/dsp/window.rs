//! Applying an FFT window to complex samples (`xdsp/fft_window_functions.c` and its generic
//! templates `fft_window_cf32_generic.t`, `fft_window_ci16_cf32_generic.t`).
//!
//! libusdr only multiplies by caller-supplied coefficients; it generates no window shapes,
//! so none are provided here either.

use num_complex::{Complex, Complex32};

/// Multiplies each sample by its window coefficient (`fft_window_cf32`): `out[i] =
/// window[i] * input[i]`, component by component, in single precision.
///
/// Processes as many samples as all three slices hold; the rest of `out` is untouched.
pub fn apply(input: &[Complex32], window: &[f32], out: &mut [Complex32]) {
    for ((sample, &coefficient), windowed) in input.iter().zip(window).zip(out) {
        *windowed = Complex32::new(coefficient * sample.re, coefficient * sample.im);
    }
}

/// Windows 16-bit integer samples into floats (`fft_window_ci16_cf32`): each component is
/// converted to `f32` exactly, then multiplied by its coefficient.
///
/// libusdr scales nothing here, so `window` must already carry any full-scale
/// normalization (`1 / 32767`, say). Processes as many samples as all three slices hold;
/// the rest of `out` is untouched.
pub fn apply_ci16(input: &[Complex<i16>], window: &[f32], out: &mut [Complex32]) {
    for ((sample, &coefficient), windowed) in input.iter().zip(window).zip(out) {
        *windowed = Complex32::new(
            coefficient * f32::from(sample.re),
            coefficient * f32::from(sample.im),
        );
    }
}
