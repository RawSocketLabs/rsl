//! The complete X25519 Montgomery ladder.
//!
//! ## Standards ownership
//!
//! [RFC 7748 §5][rfc-7748] specifies the initial projective coordinates, one ladder step for
//! each scalar bit from 254 through zero, the constant `a24 = 121665`, the final conditional
//! swap, affine recovery through `z_2^(p-2)`, and little-endian encoding. Verified Errata 7625
//! clarifies that the printed `swap ^= k_t` operation is XOR; [`scalar_multiply`] names that value
//! `swap_control ^ scalar_bit` explicitly.
//!
//! The loop count, field-operation sequence, and limb memory accesses do not depend on scalar
//! bits. This is necessary for side-channel resistance, but it is not a compiler-output or
//! platform constant-time guarantee.
//!
//! [rfc-7748]: https://www.rfc-editor.org/rfc/rfc7748.html

use super::{field::FieldElement, scalar::PreparedScalar};

/// RFC 7748's `(486662 - 2) / 4` constant for the Curve25519 ladder formula.
const A24: u64 = 121_665;

/// Calculate the raw RFC 7748 `X25519(k, u)` function.
///
/// This private primitive returns all outputs, including all zero. The public agreement boundary
/// owns contributory-behavior rejection so raw scalar multiplication cannot be mistaken for an
/// already validated shared secret.
#[must_use]
#[allow(clippy::many_single_char_names)] // `A`, `B`, `C`, `D`, and `E` are RFC 7748's names.
pub(super) fn scalar_multiply(scalar_bytes: &[u8; 32], u_bytes: &[u8; 32]) -> [u8; 32] {
    let scalar = PreparedScalar::new(scalar_bytes);
    let x_1 = FieldElement::from_bytes(u_bytes);
    let mut x_2 = FieldElement::ONE;
    let mut z_2 = FieldElement::ZERO;
    let mut x_3 = FieldElement::from_bytes(u_bytes);
    let mut z_3 = FieldElement::ONE;
    let mut swap_control = 0_u64;

    for bit_index in (0..255).rev() {
        let scalar_bit = scalar.bit(bit_index);
        swap_control ^= scalar_bit;
        FieldElement::conditional_swap(swap_control, &mut x_2, &mut x_3);
        FieldElement::conditional_swap(swap_control, &mut z_2, &mut z_3);
        swap_control = scalar_bit;

        // Names preserve RFC 7748 §5's printed ladder step exactly.
        let a = x_2.add(&z_2);
        let aa = a.square();
        let b = x_2.subtract(&z_2);
        let bb = b.square();
        let e = aa.subtract(&bb);
        let c = x_3.add(&z_3);
        let d = x_3.subtract(&z_3);
        let da = d.multiply(&a);
        let cb = c.multiply(&b);
        x_3 = da.add(&cb).square();
        z_3 = x_1.multiply(&da.subtract(&cb).square());
        x_2 = aa.multiply(&bb);
        z_2 = e.multiply(&aa.add(&e.multiply_small(A24)));
    }

    FieldElement::conditional_swap(swap_control, &mut x_2, &mut x_3);
    FieldElement::conditional_swap(swap_control, &mut z_2, &mut z_3);

    x_2.multiply(&z_2.invert()).to_bytes()
}

#[cfg(test)]
mod unit {
    use super::scalar_multiply;

    /// Published complete-function evidence from RFC 7748 §5.2, X25519 vector one.
    #[test]
    fn first_published_scalar_multiplication_vector() {
        let scalar = [
            0xa5, 0x46, 0xe3, 0x6b, 0xf0, 0x52, 0x7c, 0x9d, 0x3b, 0x16, 0x15, 0x4b, 0x82, 0x46,
            0x5e, 0xdd, 0x62, 0x14, 0x4c, 0x0a, 0xc1, 0xfc, 0x5a, 0x18, 0x50, 0x6a, 0x22, 0x44,
            0xba, 0x44, 0x9a, 0xc4,
        ];
        let input = [
            0xe6, 0xdb, 0x68, 0x67, 0x58, 0x30, 0x30, 0xdb, 0x35, 0x94, 0xc1, 0xa4, 0x24, 0xb1,
            0x5f, 0x7c, 0x72, 0x66, 0x24, 0xec, 0x26, 0xb3, 0x35, 0x3b, 0x10, 0xa9, 0x03, 0xa6,
            0xd0, 0xab, 0x1c, 0x4c,
        ];
        let expected = [
            0xc3, 0xda, 0x55, 0x37, 0x9d, 0xe9, 0xc6, 0x90, 0x8e, 0x94, 0xea, 0x4d, 0xf2, 0x8d,
            0x08, 0x4f, 0x32, 0xec, 0xcf, 0x03, 0x49, 0x1c, 0x71, 0xf7, 0x54, 0xb4, 0x07, 0x55,
            0x77, 0xa2, 0x85, 0x52,
        ];

        assert_eq!(scalar_multiply(&scalar, &input), expected);
    }
}
