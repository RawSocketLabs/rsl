//! FIPS 186-5 §6.4.1 ECDSA signature generation steps for P-256.
//!
//! The per-message secret `k` comes from the RFC 6979 generator in [`super::nonce`], which
//! FIPS 186-5 §6.3 permits as a deterministic alternative to random `k`. The signing equation
//! itself is separated into [`sign_with_nonce`] so published `(d, k, H(M)) -> (r, s)` fixtures
//! can be checked directly.

use zeroize::Zeroize;

use super::nonce::NonceGenerator;
use crate::{
    Result,
    curve::p256::{point::ProjectivePoint, scalar::Scalar},
};

/// Sign a 256-bit digest under private scalar `d`, deriving `k` deterministically.
///
/// # Errors
///
/// Propagates HMAC length errors only, which fixed-size RFC 6979 inputs cannot trigger.
pub(super) fn sign_digest(private_scalar: &[u8; 32], digest: &[u8; 32]) -> Result<[u8; 64]> {
    let d = Scalar::from_nonzero_canonical_bytes(private_scalar)
        .expect("a validated signing key holds a scalar in [1, n-1]");
    let mut generator = NonceGenerator::new(private_scalar, digest)?;

    loop {
        // RFC 6979 §3.2 h.3: reject candidates outside `[1, n-1]` without reducing them.
        let mut candidate = generator.candidate()?;
        let k = Scalar::from_nonzero_canonical_bytes(&candidate);
        candidate.zeroize();
        let outcome = k.and_then(|k| sign_with_nonce(&d, digest, &k));
        match outcome {
            Some((r, s)) => {
                let mut signature = [0_u8; 64];
                signature[..32].copy_from_slice(&r.to_bytes());
                signature[32..].copy_from_slice(&s.to_bytes());
                return Ok(signature);
            }
            None => generator.reject()?,
        }
    }
}

/// FIPS 186-5 §6.4.1 steps for one `k`; `None` when `r = 0` or `s = 0` requires a new `k`.
#[allow(clippy::many_single_char_names)] // `d`, `k`, `e`, `r`, and `s` are FIPS 186-5's names.
pub(super) fn sign_with_nonce(
    d: &Scalar,
    digest: &[u8; 32],
    k: &Scalar,
) -> Option<(Scalar, Scalar)> {
    // Step 2 (via §6.4.1 and §6.4.2 notation): `e` is the whole 256-bit digest.
    let e = Scalar::reduce_bytes(digest);

    // Step 3: `R = [k]G`; `r = x_R mod n`.
    let r_point = ProjectivePoint::generator()
        .multiply(&k.to_bytes())
        .to_affine()
        .expect("k in [1, n-1] never maps the generator to infinity");
    let r = Scalar::reduce_limbs(r_point.x().limbs());
    if r.is_zero() {
        return None;
    }

    // Step 4: `s = k^-1 (e + r d) mod n`.
    let s = k.invert().multiply(&e.add(&r.multiply(d)));
    if s.is_zero() {
        return None;
    }
    Some((r, s))
}

#[cfg(test)]
mod unit {
    use super::*;
    use crate::{digest::sha2::sha256::Sha256, signature::ecdsa_p256::cavp_siggen_fixtures::CASES};

    fn decode(hex: &str) -> [u8; 32] {
        core::array::from_fn(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
    }

    fn decode_message(hex: &str) -> alloc::vec::Vec<u8> {
        (0..hex.len() / 2)
            .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
            .collect()
    }

    /// Published evidence: all 15 CAVP `SigGen` `[P-256,SHA-256]` cases reproduce `(r, s)` from
    /// the published `d`, `k`, and message.
    #[test]
    fn cavp_siggen_cases_reproduce_r_and_s_from_the_published_nonce() {
        for case in &CASES {
            let d = Scalar::from_nonzero_canonical_bytes(&decode(case.d)).unwrap();
            let k = Scalar::from_nonzero_canonical_bytes(&decode(case.k)).unwrap();
            let digest = Sha256::digest(decode_message(case.message))
                .unwrap()
                .into_bytes();
            let (r, s) = sign_with_nonce(&d, &digest, &k).expect("published k is usable");
            assert_eq!(r.to_bytes(), decode(case.r), "r for d={}", case.d);
            assert_eq!(s.to_bytes(), decode(case.s), "s for d={}", case.d);
        }
    }
}
