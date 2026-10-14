//! Shared executable specification for prime-order short Weierstrass curves with `a = -3`.
//!
//! NIST P-256 and P-384 differ only in their parameters: limb count, field prime `p`, order `n`,
//! coefficient `b`, and generator `G`. Everything else — the limb arithmetic, the field and scalar
//! residue types, the Renes–Costello–Batina complete addition law, fixed-structure scalar
//! multiplication, and SEC 1 uncompressed encoding — is written once here, generic over the
//! [`Curve`] parameter set. [`crate::curve::p256`] and [`crate::curve::p384`] are those parameter
//! sets plus their published-vector evidence.

#![allow(rustdoc::private_intra_doc_links)]

pub(crate) mod arithmetic;
pub(crate) mod field;
pub(crate) mod point;
pub(crate) mod scalar;

use arithmetic::Modulus;

/// The SP 800-186 domain parameters of one `a = -3` prime-order curve over `N` 64-bit limbs.
pub(crate) trait Curve<const N: usize>: Copy + 'static {
    /// The field prime `p`.
    const FIELD: Modulus<N>;
    /// The prime group order `n`.
    const ORDER: Modulus<N>;
    /// `p - 2`, the public field-inversion exponent.
    const FIELD_INVERSION_EXPONENT: [u64; N];
    /// `n - 2`, the public scalar-inversion exponent.
    const ORDER_INVERSION_EXPONENT: [u64; N];
    /// Coefficient `b`.
    const B: [u64; N];
    /// Generator `x`-coordinate.
    const GENERATOR_X: [u64; N];
    /// Generator `y`-coordinate.
    const GENERATOR_Y: [u64; N];
    /// Human-readable curve name for diagnostics.
    const NAME: &'static str;
}

/// Bytes in one coordinate or scalar: `8 · N`.
pub(crate) const fn element_bytes<const N: usize>() -> usize {
    8 * N
}

/// Bytes in a SEC 1 uncompressed point: `1 + 16 · N`.
pub(crate) const fn encoded_point_bytes<const N: usize>() -> usize {
    1 + 16 * N
}
