//! Elementary SHA-256 functions over 32-bit words.
//!
//! ## Standards ownership
//!
//! This module implements
//! [NIST FIPS 180-4 §4.1.2, equations 4.2–4.7][fips-180-4]. Section 3.2 of the same standard
//! defines the bitwise operations, logical shift, and rotation notation used by those equations.
//! It does not own message parsing, schedule indexing, round ordering, or modular addition.
//!
//! ## Notation mapping
//!
//! A FIPS SHA-256 word is represented by `u32`. Rust's `&`, `^`, and `!` operators represent
//! `AND`, `XOR`, and `NOT`; `u32::rotate_right` represents `ROTR`; and `>>` on an unsigned word
//! represents `SHR`. Keeping all six functions separate and named makes the correspondence with
//! each published equation directly reviewable.
//!
//! [fips-180-4]: https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.180-4.pdf

/// Select bits from `y` where `x` is set and from `z` where `x` is clear.
///
/// **Standard mapping:** FIPS 180-4 §4.1.2, equation 4.2 names this function `Ch` and defines it as
/// `(x AND y) XOR ((NOT x) AND z)`. At each bit position, `x` therefore chooses the corresponding
/// bit from `y` when set and from `z` when clear.
///
/// **Rust mapping:** `&`, `^`, and `!` are the equation's `AND`, `XOR`, and `NOT` operations over
/// all 32 bits of each word.
#[must_use]
pub(super) const fn choose(x: u32, y: u32, z: u32) -> u32 {
    (x & y) ^ (!x & z)
}

/// Select the majority value of each corresponding bit in `x`, `y`, and `z`.
///
/// **Standard mapping:** FIPS 180-4 §4.1.2, equation 4.3 names this function `Maj` and defines it
/// as `(x AND y) XOR (x AND z) XOR (y AND z)`. The result bit is set exactly when at least two of
/// the three corresponding input bits are set.
///
/// **Rust mapping:** the three parenthesized `&` expressions remain visible in the same order as
/// the specification, joined by bitwise `^`.
#[must_use]
pub(super) const fn majority(x: u32, y: u32, z: u32) -> u32 {
    (x & y) ^ (x & z) ^ (y & z)
}

/// Apply the uppercase sigma-zero function used by the compression rounds.
///
/// **Standard mapping:** FIPS 180-4 §4.1.2, equation 4.4 defines uppercase sigma zero as the XOR
/// of `ROTR^2(x)`, `ROTR^13(x)`, and `ROTR^22(x)`.
///
/// **Rust mapping:** each `rotate_right` operates on the entire `u32`; unlike a shift, every bit
/// rotated out of the low end returns at the high end.
#[must_use]
pub(super) const fn big_sigma_0(x: u32) -> u32 {
    x.rotate_right(2) ^ x.rotate_right(13) ^ x.rotate_right(22)
}

/// Apply the uppercase sigma-one function used by the compression rounds.
///
/// **Standard mapping:** FIPS 180-4 §4.1.2, equation 4.5 defines uppercase sigma one as the XOR
/// of `ROTR^6(x)`, `ROTR^11(x)`, and `ROTR^25(x)`.
///
/// **Rust mapping:** `u32::rotate_right` fixes the operation width at the required 32 bits.
#[must_use]
pub(super) const fn big_sigma_1(x: u32) -> u32 {
    x.rotate_right(6) ^ x.rotate_right(11) ^ x.rotate_right(25)
}

/// Apply the lowercase sigma-zero function used by message-schedule expansion.
///
/// **Standard mapping:** FIPS 180-4 §4.1.2, equation 4.6 defines lowercase sigma zero as the XOR
/// of `ROTR^7(x)`, `ROTR^18(x)`, and `SHR^3(x)`.
///
/// **Rust mapping:** `rotate_right` implements the first two terms. Because `x` is an unsigned
/// `u32`, `x >> 3` is the required logical right shift and introduces zeroes at the high end. The
/// final operation is deliberately not a rotation.
#[must_use]
pub(super) const fn small_sigma_0(x: u32) -> u32 {
    x.rotate_right(7) ^ x.rotate_right(18) ^ (x >> 3)
}

/// Apply the lowercase sigma-one function used by message-schedule expansion.
///
/// **Standard mapping:** FIPS 180-4 §4.1.2, equation 4.7 defines lowercase sigma one as the XOR
/// of `ROTR^17(x)`, `ROTR^19(x)`, and `SHR^10(x)`.
///
/// **Rust mapping:** `rotate_right` implements the rotations, while unsigned `x >> 10` implements
/// the logical shift. Keeping the shift syntactically distinct makes the equation's asymmetry
/// visible during review.
#[must_use]
pub(super) const fn small_sigma_1(x: u32) -> u32 {
    x.rotate_right(17) ^ x.rotate_right(19) ^ (x >> 10)
}

#[cfg(test)]
mod unit {
    use super::{big_sigma_0, big_sigma_1, choose, majority, small_sigma_0, small_sigma_1};

    const CLEAR: u32 = 0;
    const SET: u32 = u32::MAX;

    /// Standard-derived evidence from NIST FIPS 180-4 §4.1.2, equation 4.2.
    #[test]
    fn choose_matches_its_complete_one_bit_truth_table() {
        let cases = [
            (CLEAR, CLEAR, CLEAR, CLEAR),
            (CLEAR, CLEAR, SET, SET),
            (CLEAR, SET, CLEAR, CLEAR),
            (CLEAR, SET, SET, SET),
            (SET, CLEAR, CLEAR, CLEAR),
            (SET, CLEAR, SET, CLEAR),
            (SET, SET, CLEAR, SET),
            (SET, SET, SET, SET),
        ];

        for (x, y, z, expected) in cases {
            assert_eq!(choose(x, y, z), expected);
        }
    }

    /// Standard-derived evidence from NIST FIPS 180-4 §4.1.2, equation 4.3.
    #[test]
    fn majority_matches_its_complete_one_bit_truth_table() {
        let cases = [
            (CLEAR, CLEAR, CLEAR, CLEAR),
            (CLEAR, CLEAR, SET, CLEAR),
            (CLEAR, SET, CLEAR, CLEAR),
            (CLEAR, SET, SET, SET),
            (SET, CLEAR, CLEAR, CLEAR),
            (SET, CLEAR, SET, SET),
            (SET, SET, CLEAR, SET),
            (SET, SET, SET, SET),
        ];

        for (x, y, z, expected) in cases {
            assert_eq!(majority(x, y, z), expected);
        }
    }

    /// Standard-derived evidence from NIST FIPS 180-4 §3.2 and equations 4.4–4.7.
    #[test]
    fn sigma_functions_move_the_low_bit_to_the_specified_positions() {
        let low_bit = 0x0000_0001;

        assert_eq!(big_sigma_0(low_bit), 0x4008_0400);
        assert_eq!(big_sigma_1(low_bit), 0x0420_0080);
        assert_eq!(small_sigma_0(low_bit), 0x0200_4000);
        assert_eq!(small_sigma_1(low_bit), 0x0000_a000);
    }

    /// Standard-derived evidence from NIST FIPS 180-4 §3.2 and equations 4.4–4.7.
    /// Standard-derived identity case from NIST FIPS 180-4 equations 4.4–4.7.
    #[test]
    fn lowercase_sigma_uses_a_shift_where_uppercase_sigma_uses_only_rotations() {
        assert_eq!(big_sigma_0(SET), SET);
        assert_eq!(big_sigma_1(SET), SET);
        assert_eq!(small_sigma_0(SET), 0x1fff_ffff);
        assert_eq!(small_sigma_1(SET), 0x003f_ffff);
    }

    #[test]
    fn every_sigma_function_preserves_zero() {
        assert_eq!(big_sigma_0(CLEAR), CLEAR);
        assert_eq!(big_sigma_1(CLEAR), CLEAR);
        assert_eq!(small_sigma_0(CLEAR), CLEAR);
        assert_eq!(small_sigma_1(CLEAR), CLEAR);
    }
}
