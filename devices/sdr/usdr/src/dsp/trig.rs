//! Fixed-point sine and cosine: the scalar port of libusdr's `xdsp/trig.c`
//! (`isincos_generic`), which is also the oscillator inside its NCO ([`super::nco`]).
//!
//! A 16-bit phase covers half a turn: [-π/2, π/2) maps to [-32768, 32767]. Results are Q15
//! fractions of full scale, ±32767 for ±1. The function evaluates libusdr's fixed polynomial
//! in 16-bit arithmetic with a Q15 multiply rounded half up (`MULI16_NORM`) and wrapping
//! adds, so it
//! matches the C bit for bit for every input (the oracle's `nco_parity` test checks all
//! 65536). The polynomial is libusdr's, errors included: it is not a correctly rounded sine.
//!
//! libusdr's SSSE3/AVX2 versions (`trig_inline.h`) use `pmulhrsw`, which is that multiply
//! exactly; they are not ported.

/// Sine and cosine of `phase`, where [-32768, 32767] is [-π/2, π/2): `(sin, cos)`, each in
/// Q15 (`isincos_generic`).
///
/// `-32768` (-π/2) gets libusdr's special correction (`CORR_32768`), because the rounding
/// multiply squares it to `-32768` rather than `+1`.
#[must_use]
#[expect(
    clippy::similar_names,
    reason = "trig.c's names, kept so the two read side by side"
)]
pub fn sin_cos(phase: i16) -> (i16, i16) {
    let corrected = phase == i16::MIN;
    let ph2 = mul_round(phase, phase);
    let phx1 = mul_round(phase, 18705);
    let phx3_c = mul_round(phase, -21166);
    let phx5_c = mul_round(phase, 2611);
    let phx7_c = mul_round(phase, -152);
    let ph4 = mul_round(ph2, ph2);
    let phx3 = mul_round(ph2, phx3_c);
    let phy2 = mul_round(ph2, -7656);
    let phs0 = phase.wrapping_add(phx1);
    let phc0 = 32767_i16.wrapping_sub(ph2);
    let phs1 = phs0.wrapping_add(phx3);
    let phc1 = phc0.wrapping_add(phy2);
    let ph6 = mul_round(ph4, ph2);
    let phx5 = mul_round(ph4, phx5_c);
    let phy48 = mul_round(ph4, 30);
    let phy4 = mul_round(ph4, 8311);
    let (scorr, ccorr) = if corrected { (-28123, -57) } else { (0, 0) };
    let phy6 = mul_round(ph6, -683);
    let phx7 = mul_round(ph6, phx7_c);
    let phy8 = mul_round(ph4, phy48);
    let phs2 = phs1.wrapping_add(phx5).wrapping_add(scorr);
    let phc2 = phc1.wrapping_add(phy4).wrapping_add(ccorr);
    let sin = phs2.wrapping_add(phx7);
    let cos = phc2.wrapping_add(phy6).wrapping_add(phy8);
    (sin, cos)
}

/// The Q15 product of `x` and `y`, rounded half up: libusdr's `MULI16_NORM` and x86's
/// `pmulhrsw`. As in both, `-32768 * -32768` wraps to `-32768`.
#[expect(
    clippy::cast_possible_truncation,
    reason = "C truncates the rounded product to int16_t"
)]
pub(super) fn mul_round(x: i16, y: i16) -> i16 {
    ((((i32::from(x) * i32::from(y)) >> 14) + 1) >> 1) as i16
}

#[cfg(test)]
mod tests {
    use core::f64::consts::FRAC_PI_2;

    use super::{mul_round, sin_cos};

    /// The polynomial's worst error over the whole phase range, in LSB of 32767, against
    /// the f64 sine and cosine: measured 3.9 (sine) and 3.3 (cosine) on 2026-10-12 for
    /// libusdr's coefficients. The margin only lets the bound catch a broken polynomial.
    const TOLERANCE: f64 = 6.0;

    #[test]
    fn every_phase_is_close_to_the_true_sine_and_cosine() {
        for phase in i16::MIN..=i16::MAX {
            let (sin, cos) = sin_cos(phase);
            let angle = f64::from(phase) / 32768.0 * FRAC_PI_2;
            let error_sin = (f64::from(sin) - angle.sin() * 32767.0).abs();
            let error_cos = (f64::from(cos) - angle.cos() * 32767.0).abs();
            assert!(error_sin <= TOLERANCE, "sine at {phase} off by {error_sin}");
            assert!(
                error_cos <= TOLERANCE,
                "cosine at {phase} off by {error_cos}"
            );
        }
    }

    #[test]
    fn the_rounding_multiply_is_half_up_and_wraps_only_minus_one_squared() {
        assert_eq!(mul_round(1, 16384), 1, "a half rounds up");
        assert_eq!(mul_round(-1, 16384), 0, "a negative half rounds up too");
        assert_eq!(mul_round(i16::MAX, i16::MAX), 32766);
        assert_eq!(mul_round(i16::MIN, i16::MIN), i16::MIN);
    }
}
