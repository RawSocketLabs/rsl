//! Calculated AES forward byte substitution.
//!
//! ## Standards ownership
//!
//! [NIST FIPS 197 §5.1.1][fips-197] defines `SBOX()` in two visible stages. Equation 5.2 maps a
//! nonzero byte to its multiplicative inverse in `GF(2^8)` and maps zero to zero. Equations 5.3
//! and 5.4 apply an affine bit transformation with constant byte `{63}`. Table 4 publishes all
//! 256 resulting substitutions.
//!
//! Section 5.3.2 defines `INVSBOX()` by reversing the Table 4 input/output roles and publishes all
//! 256 values in Table 6. This module calculates that inverse by undoing the affine transform and
//! then applying the field inverse. State transforms own applying the byte operations across all
//! sixteen positions.
//!
//! ## Calculation policy
//!
//! Production code calculates the field inverse and each affine output bit. The 256-byte table
//! from FIPS 197 exists only inside `#[cfg(test)]` as exhaustive published evidence; it is not
//! indexed by secret data in the cryptographic path. As with the field layer, this source-level
//! structure has not received compiler-output or platform-level constant-time analysis.
//!
//! [fips-197]: https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.197-upd1.pdf

use super::field::multiplicative_inverse_or_zero;

/// Number of bits in the byte transformed by FIPS 197 equation 5.3.
const BYTE_BITS: usize = u8::BITS as usize;

/// Affine constant `c` from FIPS 197 §5.1.1, equations 5.3 and 5.4.
///
/// The standard prints this byte as `{01100011}`, which is `0x63` with bit `c0` at the least
/// significant position and bit `c7` at the most significant position.
const AFFINE_CONSTANT: u8 = 0x63;

/// Constant in the inverse of FIPS 197 equations 5.3 and 5.4.
///
/// Solving the published affine bit matrix for its input gives a constant byte of `0x05`. This is
/// standard-derived rather than a separately numbered constant in FIPS 197; exhaustive Table 6
/// evidence below checks the complete inverse mapping.
const INVERSE_AFFINE_CONSTANT: u8 = 0x05;

/// Return one bit from a byte using FIPS 197's least-significant-bit index convention.
///
/// FIPS 197 §3.2 labels a byte `{b7 b6 b5 b4 b3 b2 b1 b0}`. Shifting right by `bit_index` moves
/// `b[bit_index]` to position zero; masking discards every other coefficient.
#[must_use]
const fn bit(byte: u8, bit_index: usize) -> u8 {
    (byte >> bit_index) & 1
}

/// Apply the affine bit transformation from FIPS 197 equations 5.3 and 5.4.
///
/// For each output bit index `i`, equation 5.3 combines inverse bits `i`, `i+4`, `i+5`, `i+6`,
/// and `i+7`, with every offset reduced modulo eight, then adds constant bit `c_i`. The loop and
/// named terms intentionally preserve that printed equation instead of replacing it with a less
/// obvious rotation identity.
#[must_use]
const fn affine_transform(inverse: u8) -> u8 {
    let mut transformed = 0_u8;
    let mut output_bit_index = 0;

    while output_bit_index < BYTE_BITS {
        let same_position = bit(inverse, output_bit_index);
        let offset_four = bit(inverse, (output_bit_index + 4) % BYTE_BITS);
        let offset_five = bit(inverse, (output_bit_index + 5) % BYTE_BITS);
        let offset_six = bit(inverse, (output_bit_index + 6) % BYTE_BITS);
        let offset_seven = bit(inverse, (output_bit_index + 7) % BYTE_BITS);
        let constant_bit = bit(AFFINE_CONSTANT, output_bit_index);
        let output_bit =
            same_position ^ offset_four ^ offset_five ^ offset_six ^ offset_seven ^ constant_bit;

        transformed |= output_bit << output_bit_index;
        output_bit_index += 1;
    }

    transformed
}

/// Apply the AES forward S-box to one byte.
///
/// **Standard mapping:** FIPS 197 equation 5.2 supplies the field inverse (with the explicit zero
/// case), and equation 5.3 transforms that inverse into the substituted byte. Keeping these two
/// calls separate preserves the composition that the standard describes.
#[must_use]
pub(super) const fn substitute_byte(byte: u8) -> u8 {
    let inverse = multiplicative_inverse_or_zero(byte);

    affine_transform(inverse)
}

/// Undo the forward S-box affine bit transformation.
///
/// **Standard-derived mapping:** algebraically inverting the binary matrix in FIPS 197 equation
/// 5.4 gives `ROTL1(b) XOR ROTL3(b) XOR ROTL6(b) XOR {05}`. Rust's `u8::rotate_left` keeps all
/// eight coefficients and makes wraparound explicit. This helper is not presented as a separately
/// published FIPS equation; all 256 results are validated against the standard's Table 6.
#[must_use]
const fn inverse_affine_transform(byte: u8) -> u8 {
    byte.rotate_left(1) ^ byte.rotate_left(3) ^ byte.rotate_left(6) ^ INVERSE_AFFINE_CONSTANT
}

/// Apply the AES inverse S-box to one byte without a production lookup table.
///
/// **Standard mapping:** FIPS 197 §5.3.2 defines `INVSBOX()` as the inverse of `SBOX()` and
/// publishes its values in Table 6. Reversing the forward composition first undoes the affine
/// transform, then uses the self-inverse field-inversion operation from equation 5.2. Zero follows
/// the same explicit zero-to-zero field boundary.
#[must_use]
pub(super) const fn inverse_substitute_byte(byte: u8) -> u8 {
    let inverse_affine = inverse_affine_transform(byte);

    multiplicative_inverse_or_zero(inverse_affine)
}

#[cfg(test)]
mod unit {
    use super::{
        AFFINE_CONSTANT, affine_transform, inverse_affine_transform, inverse_substitute_byte,
        substitute_byte,
    };

    /// All published substitution values from FIPS 197-upd1 §5.1.1, Table 4.
    ///
    /// Array index is the input byte `xy`: the high nibble `x` selects the printed table row and
    /// the low nibble `y` selects the printed column. This table is test evidence only and is not
    /// compiled into the production substitution path.
    const PUBLISHED_SBOX: [u8; 256] = [
        0x63, 0x7c, 0x77, 0x7b, 0xf2, 0x6b, 0x6f, 0xc5, 0x30, 0x01, 0x67, 0x2b, 0xfe, 0xd7, 0xab,
        0x76, 0xca, 0x82, 0xc9, 0x7d, 0xfa, 0x59, 0x47, 0xf0, 0xad, 0xd4, 0xa2, 0xaf, 0x9c, 0xa4,
        0x72, 0xc0, 0xb7, 0xfd, 0x93, 0x26, 0x36, 0x3f, 0xf7, 0xcc, 0x34, 0xa5, 0xe5, 0xf1, 0x71,
        0xd8, 0x31, 0x15, 0x04, 0xc7, 0x23, 0xc3, 0x18, 0x96, 0x05, 0x9a, 0x07, 0x12, 0x80, 0xe2,
        0xeb, 0x27, 0xb2, 0x75, 0x09, 0x83, 0x2c, 0x1a, 0x1b, 0x6e, 0x5a, 0xa0, 0x52, 0x3b, 0xd6,
        0xb3, 0x29, 0xe3, 0x2f, 0x84, 0x53, 0xd1, 0x00, 0xed, 0x20, 0xfc, 0xb1, 0x5b, 0x6a, 0xcb,
        0xbe, 0x39, 0x4a, 0x4c, 0x58, 0xcf, 0xd0, 0xef, 0xaa, 0xfb, 0x43, 0x4d, 0x33, 0x85, 0x45,
        0xf9, 0x02, 0x7f, 0x50, 0x3c, 0x9f, 0xa8, 0x51, 0xa3, 0x40, 0x8f, 0x92, 0x9d, 0x38, 0xf5,
        0xbc, 0xb6, 0xda, 0x21, 0x10, 0xff, 0xf3, 0xd2, 0xcd, 0x0c, 0x13, 0xec, 0x5f, 0x97, 0x44,
        0x17, 0xc4, 0xa7, 0x7e, 0x3d, 0x64, 0x5d, 0x19, 0x73, 0x60, 0x81, 0x4f, 0xdc, 0x22, 0x2a,
        0x90, 0x88, 0x46, 0xee, 0xb8, 0x14, 0xde, 0x5e, 0x0b, 0xdb, 0xe0, 0x32, 0x3a, 0x0a, 0x49,
        0x06, 0x24, 0x5c, 0xc2, 0xd3, 0xac, 0x62, 0x91, 0x95, 0xe4, 0x79, 0xe7, 0xc8, 0x37, 0x6d,
        0x8d, 0xd5, 0x4e, 0xa9, 0x6c, 0x56, 0xf4, 0xea, 0x65, 0x7a, 0xae, 0x08, 0xba, 0x78, 0x25,
        0x2e, 0x1c, 0xa6, 0xb4, 0xc6, 0xe8, 0xdd, 0x74, 0x1f, 0x4b, 0xbd, 0x8b, 0x8a, 0x70, 0x3e,
        0xb5, 0x66, 0x48, 0x03, 0xf6, 0x0e, 0x61, 0x35, 0x57, 0xb9, 0x86, 0xc1, 0x1d, 0x9e, 0xe1,
        0xf8, 0x98, 0x11, 0x69, 0xd9, 0x8e, 0x94, 0x9b, 0x1e, 0x87, 0xe9, 0xce, 0x55, 0x28, 0xdf,
        0x8c, 0xa1, 0x89, 0x0d, 0xbf, 0xe6, 0x42, 0x68, 0x41, 0x99, 0x2d, 0x0f, 0xb0, 0x54, 0xbb,
        0x16,
    ];

    /// All published inverse-substitution values from FIPS 197-upd1 §5.3.2, Table 6.
    ///
    /// Indexing follows the same high-nibble row and low-nibble column convention as Table 4.
    /// This table is compiled only as test evidence.
    const PUBLISHED_INVERSE_SBOX: [u8; 256] = [
        0x52, 0x09, 0x6a, 0xd5, 0x30, 0x36, 0xa5, 0x38, 0xbf, 0x40, 0xa3, 0x9e, 0x81, 0xf3, 0xd7,
        0xfb, 0x7c, 0xe3, 0x39, 0x82, 0x9b, 0x2f, 0xff, 0x87, 0x34, 0x8e, 0x43, 0x44, 0xc4, 0xde,
        0xe9, 0xcb, 0x54, 0x7b, 0x94, 0x32, 0xa6, 0xc2, 0x23, 0x3d, 0xee, 0x4c, 0x95, 0x0b, 0x42,
        0xfa, 0xc3, 0x4e, 0x08, 0x2e, 0xa1, 0x66, 0x28, 0xd9, 0x24, 0xb2, 0x76, 0x5b, 0xa2, 0x49,
        0x6d, 0x8b, 0xd1, 0x25, 0x72, 0xf8, 0xf6, 0x64, 0x86, 0x68, 0x98, 0x16, 0xd4, 0xa4, 0x5c,
        0xcc, 0x5d, 0x65, 0xb6, 0x92, 0x6c, 0x70, 0x48, 0x50, 0xfd, 0xed, 0xb9, 0xda, 0x5e, 0x15,
        0x46, 0x57, 0xa7, 0x8d, 0x9d, 0x84, 0x90, 0xd8, 0xab, 0x00, 0x8c, 0xbc, 0xd3, 0x0a, 0xf7,
        0xe4, 0x58, 0x05, 0xb8, 0xb3, 0x45, 0x06, 0xd0, 0x2c, 0x1e, 0x8f, 0xca, 0x3f, 0x0f, 0x02,
        0xc1, 0xaf, 0xbd, 0x03, 0x01, 0x13, 0x8a, 0x6b, 0x3a, 0x91, 0x11, 0x41, 0x4f, 0x67, 0xdc,
        0xea, 0x97, 0xf2, 0xcf, 0xce, 0xf0, 0xb4, 0xe6, 0x73, 0x96, 0xac, 0x74, 0x22, 0xe7, 0xad,
        0x35, 0x85, 0xe2, 0xf9, 0x37, 0xe8, 0x1c, 0x75, 0xdf, 0x6e, 0x47, 0xf1, 0x1a, 0x71, 0x1d,
        0x29, 0xc5, 0x89, 0x6f, 0xb7, 0x62, 0x0e, 0xaa, 0x18, 0xbe, 0x1b, 0xfc, 0x56, 0x3e, 0x4b,
        0xc6, 0xd2, 0x79, 0x20, 0x9a, 0xdb, 0xc0, 0xfe, 0x78, 0xcd, 0x5a, 0xf4, 0x1f, 0xdd, 0xa8,
        0x33, 0x88, 0x07, 0xc7, 0x31, 0xb1, 0x12, 0x10, 0x59, 0x27, 0x80, 0xec, 0x5f, 0x60, 0x51,
        0x7f, 0xa9, 0x19, 0xb5, 0x4a, 0x0d, 0x2d, 0xe5, 0x7a, 0x9f, 0x93, 0xc9, 0x9c, 0xef, 0xa0,
        0xe0, 0x3b, 0x4d, 0xae, 0x2a, 0xf5, 0xb0, 0xc8, 0xeb, 0xbb, 0x3c, 0x83, 0x53, 0x99, 0x61,
        0x17, 0x2b, 0x04, 0x7e, 0xba, 0x77, 0xd6, 0x26, 0xe1, 0x69, 0x14, 0x63, 0x55, 0x21, 0x0c,
        0x7d,
    ];

    /// Published boundary evidence from the first cell of FIPS 197-upd1 Table 4.
    ///
    /// Equation 5.2 maps zero to zero, so the affine constant itself becomes the S-box output.
    #[test]
    fn zero_maps_to_the_affine_constant() {
        assert_eq!(affine_transform(0x00), AFFINE_CONSTANT);
        assert_eq!(substitute_byte(0x00), 0x63);
    }

    /// Published example from the prose immediately following FIPS 197-upd1 Table 4.
    #[test]
    fn table_lookup_example_53_maps_to_ed_without_a_production_table() {
        assert_eq!(substitute_byte(0x53), 0xed);
    }

    /// Exhaustive published-value evidence from FIPS 197-upd1 Table 4.
    #[test]
    fn calculated_substitution_matches_every_published_table_entry() {
        for (input, expected) in (u8::MIN..=u8::MAX).zip(PUBLISHED_SBOX) {
            assert_eq!(substitute_byte(input), expected, "input {input:#04x}");
        }
    }

    /// Standard-derived bijection evidence for the one-to-one substitution required by §5.1.1.
    #[test]
    fn every_output_byte_occurs_exactly_once() {
        let mut seen = [false; 256];

        for input in u8::MIN..=u8::MAX {
            let output = usize::from(substitute_byte(input));
            assert!(!seen[output], "duplicate output {output:#04x}");
            seen[output] = true;
        }

        assert!(seen.into_iter().all(core::convert::identity));
    }

    /// Standard-derived inverse-affine boundary from FIPS 197 equations 5.3–5.4.
    #[test]
    fn inverse_affine_transform_undoes_the_forward_transform_for_every_byte() {
        for byte in u8::MIN..=u8::MAX {
            assert_eq!(inverse_affine_transform(affine_transform(byte)), byte);
        }
    }

    /// Exhaustive published-value evidence from FIPS 197-upd1 Table 6.
    #[test]
    fn calculated_inverse_substitution_matches_every_published_table_entry() {
        for (input, expected) in (u8::MIN..=u8::MAX).zip(PUBLISHED_INVERSE_SBOX) {
            assert_eq!(
                inverse_substitute_byte(input),
                expected,
                "input {input:#04x}"
            );
        }
    }

    /// Standard-derived bijection evidence from FIPS 197-upd1 §§5.1.1 and 5.3.2.
    #[test]
    fn forward_and_inverse_substitutions_cancel_for_every_byte() {
        for byte in u8::MIN..=u8::MAX {
            assert_eq!(inverse_substitute_byte(substitute_byte(byte)), byte);
            assert_eq!(substitute_byte(inverse_substitute_byte(byte)), byte);
        }
    }
}
