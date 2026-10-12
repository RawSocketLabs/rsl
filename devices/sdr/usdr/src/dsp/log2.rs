//! libusdr's approximate base-2 logarithm (`xdsp/fast_math.h::wvlt_polylog2f`), shared by
//! the spectrum accumulators and the RTSA.

/// `wvlt_polylog2f`: the float's unbiased exponent plus a cubic minimax fit of
/// `log2(m) / (m - 1)` times `m - 1` over its mantissa `m` in [1, 2).
///
/// The sign bit is ignored, so a negative input yields the logarithm of its magnitude.
/// Zero yields -127 and infinities 128; a NaN's mantissa makes it finite too. Each step is
/// the C's float operation, in its order, so the result is libusdr's bit for bit.
#[expect(
    clippy::excessive_precision,
    reason = "libusdr's literals, which round to the same floats"
)]
#[expect(
    clippy::cast_precision_loss,
    reason = "the exponent field is below 2^8, exact in f32"
)]
pub(super) fn polylog2(x: f32) -> f32 {
    const EXPONENT: u32 = 0x7f80_0000;
    const MANTISSA: u32 = 0x007f_ffff;
    let bits = x.to_bits();
    // C: (float)(((i & exp) >> 23) - 127) in int32_t; the field is 0..=255, so the
    // difference is exact either way.
    let exponent = ((bits & EXPONENT) >> 23) as f32 - 127.0;
    let mantissa = f32::from_bits((bits & MANTISSA) | 1.0_f32.to_bits());
    let poly = mantissa
        * (mantissa * 0.204_446_009_836_232_697_516_f32 - 1.049_130_552_173_401_241_91_f32)
        + 2.283_302_844_769_184_906_82_f32;
    poly * (mantissa - 1.0) + exponent
}

#[cfg(test)]
mod tests {
    use super::polylog2;

    #[test]
    fn exact_powers_of_two_have_exact_logarithms() {
        for power in -126..=127 {
            let x = 2.0_f32.powi(power);
            #[expect(clippy::cast_precision_loss, reason = "small integers")]
            let expected = power as f32;
            assert_eq!(polylog2(x).to_bits(), expected.to_bits(), "log2(2^{power})");
        }
    }

    #[test]
    fn approximation_stays_within_its_fit_error() {
        // The fit's own error peaks at 2.83e-3 over [1, 2] (evaluated in double on a 1e-5
        // grid); 3e-3 leaves float rounding margin and still catches a wrong coefficient.
        for step in 0..=1000_u16 {
            let x = 1.0 + f32::from(step) / 1000.0;
            let error = (polylog2(x) - x.log2()).abs();
            assert!(error < 3e-3, "log2({x}) is off by {error}");
        }
    }

    #[test]
    fn sign_is_ignored_and_special_values_are_finite() {
        assert_eq!(polylog2(-8.0).to_bits(), polylog2(8.0).to_bits());
        assert_eq!(polylog2(0.0).to_bits(), (-127.0_f32).to_bits());
        assert_eq!(polylog2(f32::INFINITY).to_bits(), 128.0_f32.to_bits());
        assert!(polylog2(f32::NAN).is_finite());
    }
}
