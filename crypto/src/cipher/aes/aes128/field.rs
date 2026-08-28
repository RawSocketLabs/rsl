//! Byte arithmetic in AES's finite field `GF(2^8)`.
//!
//! ## Standards ownership
//!
//! [NIST FIPS 197 §4.1][fips-197] represents a byte as a polynomial whose coefficients are its
//! bits and defines field addition as bitwise exclusive-or. Section 4.2, equations 4.3 and 4.4,
//! defines multiplication as polynomial multiplication reduced modulo
//! `m(x) = x^8 + x^4 + x^3 + x + 1`. Equation 4.5 names multiplication by `{02}` as
//! `XTIMES()`. Section 4.4, equation 4.11, permits calculating every nonzero inverse as `b^254`.
//!
//! This module owns arithmetic on individual bytes. It does not own §4.3's fixed word matrices,
//! §5.1's state transforms, or the affine part of the AES S-box. Those layers consume these
//! operations without changing their definitions.
//!
//! ## Calculation policy
//!
//! The readable reference path performs a fixed number of bit operations instead of indexing a
//! secret-dependent multiplication or inverse table. Masks select whether a polynomial term is
//! included. This source has not undergone compiler-output or platform-level constant-time
//! analysis, so the absence of source-level secret branches is not a side-channel-resistance
//! claim.
//!
//! [fips-197]: https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.197-upd1.pdf

/// The low eight coefficients of FIPS 197 equation 4.3's irreducible polynomial.
///
/// When a left shift produces an `x^8` term, reduction removes that term and adds
/// `x^4 + x^3 + x + 1`. Those coefficients are the byte `0001_1011`, or `0x1b`, used in
/// equation 4.5. The implicit `x^8` coefficient does not fit in this `u8` constant.
const REDUCTION_POLYNOMIAL_LOW_BYTE: u8 = 0x1b;

/// Add two elements of `GF(2^8)`.
///
/// **Standard mapping:** FIPS 197 §4.1 and equation 4.2 define coefficient addition modulo two
/// as bitwise exclusive-or. Every `u8` bit is one polynomial coefficient, so Rust's `^` performs
/// all eight coefficient additions independently.
#[must_use]
pub(super) const fn add(left: u8, right: u8) -> u8 {
    left ^ right
}

/// Multiply one field element by `{02}`, the polynomial `x`.
///
/// **Standard mapping:** FIPS 197 equation 4.5 shifts the low seven bits left and conditionally
/// adds `0x1b` when the original high coefficient `b7` was one. `high_bit` is therefore either
/// zero or one. Subtracting it from zero with wrapping arithmetic produces either `0x00` or
/// `0xff`, allowing bitwise selection of the reduction coefficients without a value-dependent
/// branch.
#[must_use]
pub(super) const fn xtimes(byte: u8) -> u8 {
    let high_bit = byte >> 7;
    let reduction_mask = 0_u8.wrapping_sub(high_bit);

    (byte << 1) ^ (REDUCTION_POLYNOMIAL_LOW_BYTE & reduction_mask)
}

/// Multiply two elements of `GF(2^8)`.
///
/// **Standard mapping:** FIPS 197 equations 4.3 and 4.4 define polynomial multiplication modulo
/// `m(x)`. Each of the multiplier's eight bits selects one successively doubled multiple of the
/// multiplicand. [`xtimes`] performs that doubling with immediate modular reduction, which §4.2
/// explicitly permits. Exclusive-or accumulates the selected polynomial terms.
///
/// The loop always performs eight iterations. `selection_mask` is `0xff` when the current
/// multiplier coefficient is one and `0x00` otherwise, so selection does not branch on either
/// operand at the Rust source level.
#[must_use]
pub(super) const fn multiply(multiplicand: u8, multiplier: u8) -> u8 {
    let mut product = 0_u8;
    let mut current_multiple = multiplicand;
    let mut remaining_multiplier = multiplier;
    let mut bit_index = 0;

    while bit_index < u8::BITS {
        let coefficient = remaining_multiplier & 1;
        let selection_mask = 0_u8.wrapping_sub(coefficient);

        product ^= current_multiple & selection_mask;
        current_multiple = xtimes(current_multiple);
        remaining_multiplier >>= 1;
        bit_index += 1;
    }

    product
}

/// Calculate the multiplicative inverse of a byte, mapping zero to zero for the S-box boundary.
///
/// **Standard mapping:** FIPS 197 equation 4.11 gives `b^-1 = b^254` for every nonzero field
/// element. The named squarings calculate `b^2`, `b^4`, through `b^128`; multiplying all powers
/// from 2 through 128 produces exponent `2 + 4 + 8 + 16 + 32 + 64 + 128 = 254`.
///
/// Section 4.4 does not define an inverse for zero. FIPS 197 equation 5.2 explicitly maps zero to
/// zero before the S-box affine transform, and exponentiation through [`multiply`] naturally
/// gives that value. Naming the zero behavior here prevents a caller from mistaking it for a
/// mathematical inverse.
#[must_use]
#[allow(
    clippy::similar_names,
    reason = "power names intentionally expose every exponent in the FIPS 197 b^254 chain"
)]
pub(super) const fn multiplicative_inverse_or_zero(byte: u8) -> u8 {
    let power_2 = multiply(byte, byte);
    let power_4 = multiply(power_2, power_2);
    let power_8 = multiply(power_4, power_4);
    let power_16 = multiply(power_8, power_8);
    let power_32 = multiply(power_16, power_16);
    let power_64 = multiply(power_32, power_32);
    let power_128 = multiply(power_64, power_64);

    let power_6 = multiply(power_2, power_4);
    let power_14 = multiply(power_6, power_8);
    let power_30 = multiply(power_14, power_16);
    let power_62 = multiply(power_30, power_32);
    let power_126 = multiply(power_62, power_64);

    multiply(power_126, power_128)
}

#[cfg(test)]
mod unit {
    use super::{add, multiplicative_inverse_or_zero, multiply, xtimes};

    /// Published arithmetic evidence from FIPS 197-upd1 §4.1, equation 4.2.
    #[test]
    fn addition_matches_the_published_polynomial_binary_and_hex_example() {
        assert_eq!(add(0x57, 0x83), 0xd4);
    }

    /// Published arithmetic evidence from FIPS 197-upd1 §4.2, equation 4.6.
    #[test]
    fn repeated_xtimes_matches_every_published_power_of_x_for_57() {
        let mut value = 0x57;

        for expected in [0xae, 0x47, 0x8e, 0x07, 0x0e, 0x1c, 0x38] {
            value = xtimes(value);
            assert_eq!(value, expected);
        }
    }

    /// Published arithmetic evidence from FIPS 197-upd1 §4.2, equation 4.7.
    #[test]
    fn general_multiplication_matches_the_published_57_times_13_example() {
        assert_eq!(multiply(0x57, 0x13), 0xfe);
    }

    /// Standard-derived evidence from FIPS 197-upd1 equations 4.4 and 4.5.
    ///
    /// This exhaustively checks that general multiplication by `{02}` follows the separately
    /// specified `XTIMES()` operation for every field element. The expectations are derived from
    /// the published relationship rather than imported as 256 published vectors.
    #[test]
    fn multiplication_by_two_equals_xtimes_for_every_byte() {
        for byte in u8::MIN..=u8::MAX {
            assert_eq!(multiply(byte, 0x02), xtimes(byte), "byte {byte:#04x}");
        }
    }

    /// Standard-derived evidence from the field identities in FIPS 197 §4.1–§4.2.
    #[test]
    fn zero_and_one_have_the_required_multiplication_behavior() {
        for byte in u8::MIN..=u8::MAX {
            assert_eq!(multiply(byte, 0x00), 0x00, "right zero for {byte:#04x}");
            assert_eq!(multiply(0x00, byte), 0x00, "left zero for {byte:#04x}");
            assert_eq!(multiply(byte, 0x01), byte, "right one for {byte:#04x}");
            assert_eq!(multiply(0x01, byte), byte, "left one for {byte:#04x}");
        }
    }

    /// Standard-derived exhaustive evidence from FIPS 197-upd1 §4.4, equations 4.10 and 4.11.
    ///
    /// The standard publishes the exponent rule, not a table of all expected inverse bytes. This
    /// test checks the defining inverse equation independently for all 255 nonzero elements.
    #[test]
    fn every_nonzero_byte_times_its_calculated_inverse_is_one() {
        for byte in 1..=u8::MAX {
            let inverse = multiplicative_inverse_or_zero(byte);

            assert_eq!(multiply(byte, inverse), 0x01, "byte {byte:#04x}");
        }
    }

    /// Standard-derived boundary evidence from FIPS 197-upd1 equation 5.2.
    #[test]
    fn zero_maps_to_zero_at_the_future_sbox_inverse_boundary() {
        assert_eq!(multiplicative_inverse_or_zero(0x00), 0x00);
    }
}
