//! C's conversions between integers and `double`, as the clock chips' solvers use them.
//! Rust's `as` matches them on the values the solvers produce (non-negative, in range);
//! each helper names the C operation it stands for.
//!
//! The solvers also call `ceil`, `floor` and `log2` on quotients of integers. They use
//! exact integer forms instead (`div_ceil`, integer division, the smallest power of two at
//! or above), which agree with libm's here: a quotient of two integers below 2^35 is either
//! a whole number (or power of two), which `double` represents and libm returns exactly, or
//! at least 2^-35 of itself away from one, far beyond the one-ulp (2^-52) rounding a `double`
//! quotient or a `log2` can add. Each use cites this.

/// C's conversion of a 64-bit integer to `double`, exact below 2^53, where the solvers'
/// frequencies (at most 22.6 GHz) and divider products stay.
#[expect(
    clippy::cast_precision_loss,
    reason = "C's integer-to-double conversion, on values below 2^53"
)]
pub(crate) const fn c_f64(value: u64) -> f64 {
    value as f64
}

/// C's `(uint64_t)` conversion of a non-negative double below 2^64: truncation.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "C's double-to-integer truncation, on values the C keeps in range"
)]
pub(crate) const fn c_u64(value: f64) -> u64 {
    value as u64
}

/// C's `(unsigned)` conversion of a non-negative double below 2^32: truncation.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "C's double-to-unsigned truncation, on values the C keeps in range"
)]
pub(crate) const fn c_unsigned(value: f64) -> u32 {
    value as u32
}

/// C's `fabs`: the sign bit cleared, nothing else changed.
pub(crate) const fn abs(value: f64) -> f64 {
    f64::from_bits(value.to_bits() & !(1 << 63))
}

/// The greatest common divisor (libusdr's `find_gcd`); zero only for `gcd(0, 0)`.
pub(crate) const fn gcd(a: u64, b: u64) -> u64 {
    let (mut a, mut b) = (a, b);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// `ceil(log2((double)target / unit))` for whole `unit` and `target`, `unit` > 0: the
/// smallest `k` with `unit << k` at or above `target` (see the module docs).
pub(crate) const fn ceil_log2_ratio(target: u64, unit: u64) -> u32 {
    let mut k = 0;
    while unit << k < target {
        k += 1;
    }
    k
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_integer_forms_agree_with_libm_on_integer_quotients() {
        for (target, unit) in [
            (5_650_000_000, 1_000_000_000),
            (5_650_000_000, 353_125_000),
            (5_650_000_000, 353_124_999),
            (5_650_000_000, 45_000_000),
            (1_100_000_000, 550_000_000),
            (16, 1),
            (17, 1),
        ] {
            let quotient = c_f64(target) / c_f64(unit);
            assert_eq!(
                f64::from(ceil_log2_ratio(target, unit)),
                quotient.log2().ceil(),
                "ceil(log2({target} / {unit}))"
            );
        }
        for value in [0.0, 1.5, -2.25, 1e300] {
            assert_eq!(abs(value).to_bits(), value.abs().to_bits(), "fabs({value})");
        }
        assert_eq!(
            (gcd(0, 0), gcd(0, 9), gcd(200_000_000, 300_000_000)),
            (0, 9, 100_000_000)
        );
    }
}
