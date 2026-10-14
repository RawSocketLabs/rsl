//! FIPS 186-5 §6.4.2 ECDSA signature verification steps for P-384.
//!
//! This function receives a validated point and an already computed digest so the sequence
//! matches the standard's numbered steps rather than any particular message API.

use crate::{
    CryptoError, Result,
    curve::p384::{AffinePoint, ProjectivePoint, Scalar},
};

/// Verify `(r, s)` over the 256-bit digest `e` under public point `q`.
///
/// # Errors
///
/// Returns [`CryptoError::InvalidSignature`] for an out-of-range `r` or `s`, a reconstructed
/// point at infinity, or `x_R mod n != r`.
#[allow(clippy::many_single_char_names)] // `q`, `e`, `w`, `r`, and `s` are FIPS 186-5's names.
pub(super) fn verify_digest(
    q: &AffinePoint,
    digest: &[u8; 48],
    r_bytes: &[u8; 48],
    s_bytes: &[u8; 48],
) -> Result<()> {
    // Step 1: `r` and `s` must both lie in `[1, n-1]`.
    let r = Scalar::from_nonzero_canonical_bytes(r_bytes).ok_or(CryptoError::InvalidSignature)?;
    let s = Scalar::from_nonzero_canonical_bytes(s_bytes).ok_or(CryptoError::InvalidSignature)?;

    // Step 2: `e` is the leftmost `min(N, outlen)` bits of `H(M)`; with SHA-384 that is all 256
    // bits, reduced modulo `n` on entry to the scalar domain.
    let e = Scalar::reduce_bytes(digest).expect("a SHA-384 digest is exactly 32 bytes");

    // Step 3: `w = s^-1 mod n`.
    let w = s.invert();

    // Step 4: `u1 = e w mod n`, `u2 = r w mod n`.
    let u1 = e.multiply(&w);
    let u2 = r.multiply(&w);

    // Step 5: `R = [u1]G + [u2]Q`; reject the point at infinity.
    let mut u1_bytes = [0_u8; 48];
    u1.write_bytes(&mut u1_bytes);
    let mut u2_bytes = [0_u8; 48];
    u2.write_bytes(&mut u2_bytes);
    let r_point = ProjectivePoint::generator()
        .multiply(&u1_bytes)
        .add(&q.to_projective().multiply(&u2_bytes));
    let r_affine = r_point.to_affine().ok_or(CryptoError::InvalidSignature)?;

    // Step 6: `v = x_R mod n`; accept exactly when `v == r`.
    let v = Scalar::reduce_limbs(r_affine.x().limbs());
    if v.equals(&r) {
        Ok(())
    } else {
        Err(CryptoError::InvalidSignature)
    }
}
