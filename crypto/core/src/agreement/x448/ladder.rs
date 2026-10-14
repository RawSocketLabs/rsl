//! The RFC 7748 §5 Montgomery ladder instantiated for X448.
//!
//! The ladder body is identical to X25519's; only the field, `a24 = 39081`, the 448-bit loop
//! bound, and the scalar preparation differ. Names preserve the RFC's printed step names.

use super::{field::FieldElement, scalar::PreparedScalar};

/// `(A - 2) / 4` for curve448's `A = 156326`.
const A24: u64 = 39_081;

/// `X448(k, u)` over 56-byte little-endian encodings.
#[must_use]
#[allow(clippy::many_single_char_names)] // `A`, `B`, `C`, `D`, and `E` are RFC 7748's names.
pub(super) fn scalar_multiply(scalar_bytes: &[u8; 56], u_bytes: &[u8; 56]) -> [u8; 56] {
    let scalar = PreparedScalar::new(scalar_bytes);
    let x_1 = FieldElement::from_bytes(u_bytes);
    let mut x_2 = FieldElement::ONE;
    let mut z_2 = FieldElement::ZERO;
    let mut x_3 = FieldElement::from_bytes(u_bytes);
    let mut z_3 = FieldElement::ONE;
    let mut swap_control = 0_u64;

    for bit_index in (0..448).rev() {
        let scalar_bit = scalar.bit(bit_index);
        swap_control ^= scalar_bit;
        FieldElement::conditional_swap(swap_control, &mut x_2, &mut x_3);
        FieldElement::conditional_swap(swap_control, &mut z_2, &mut z_3);
        swap_control = scalar_bit;

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
