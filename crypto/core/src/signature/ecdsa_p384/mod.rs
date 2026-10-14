//! ECDSA over P-384 with SHA-384, taught from a digest to a checked point.
//!
//! # What ECDSA P-384 does
//!
//! A signer with private scalar `d` and public point `Q = [d]G` produces `(r, s)` over the
//! digest of a message using a per-signature secret `k`. A verifier reconstructs a point from
//! the signature and the digest and accepts exactly when that point's `x`-coordinate, reduced
//! modulo `n`, equals `r`. FIPS 186-5 §6.4.1 and §6.4.2 define signing and verification;
//! SP 800-186 supplies the curve. TLS 1.3 names this profile `ecdsa_secp384r1_sha384`; SSH
//! names it `ecdsa-sha2-nistp384`.
//!
//! Signing here is deterministic: `k` is derived from the private key and digest by RFC 6979
//! §3.2, which FIPS 186-5 §6.3 permits. A repeated or biased `k` reveals the private key, and
//! deterministic derivation removes that dependency on the caller's entropy source.
//!
//! # Inputs, output, and checked behavior
//!
//! - [`EcdsaP384SigningKey`] owns one scalar in `[1, n-1]`; construction rejects zero and
//!   values `>= n`. Generation follows FIPS 186-5 Appendix A.2.2 candidate testing.
//! - [`EcdsaP384VerifyingKey`] owns a fully validated 97-byte uncompressed point.
//! - [`EcdsaP384Signature`] owns the fixed 96-byte encoding `r || s`, both big-endian. Range
//!   checks happen at verification so a detached wire field can be parsed on its own.
//! - Verification returns [`CryptoError::InvalidSignature`](crate::CryptoError::InvalidSignature)
//!   for `r` or `s` outside `[1, n-1]`, for a reconstructed point at infinity, and for a failed
//!   comparison.
//!
//! # Standards notation in Rust
//!
//! | FIPS 186-5 / RFC 6979 name | Rust representation | Meaning |
//! | --- | --- | --- |
//! | `d`, `x` | [`EcdsaP384SigningKey`] | The private scalar. |
//! | `Q` | [`EcdsaP384VerifyingKey`] | The signer's public point. |
//! | `(r, s)` | [`EcdsaP384Signature`] | The signature, two scalars in `[1, n-1]`. |
//! | `H(M)`, `e`, `h1` | [`Sha384Digest`](crate::digest::sha2::sha384::Sha384Digest) then `Scalar::reduce_bytes` | The 256-bit digest as an integer modulo `n`. |
//! | `K`, `V`, `T` | private [`nonce::NonceGenerator`] | RFC 6979 HMAC-SHA-384 state and candidate. |
//! | `k` | local `Scalar` in [`sign::sign_digest`] | The per-signature secret. |
//! | `s = k^-1 (e + r d)` | `Scalar::invert`, `add`, `multiply` | The signing equation. |
//! | `w = s^-1 mod n` | `Scalar::invert` | Inverse of `s` for verification. |
//! | `u1 = e w`, `u2 = r w` | `Scalar::multiply` | Verification multipliers. |
//! | `R = [u1]G + [u2]Q` | `ProjectivePoint::multiply` and `add` | The reconstructed point. |
//! | `v = x_R mod n` | `Scalar::reduce_limbs` | The candidate for `r`. |
//!
//! # Signing walkthrough
//!
//! 1. Hash the message with SHA-384 to `h1`; because the digest and `n` are both 384 bits,
//!    RFC 6979's `bits2int` keeps the whole digest and `bits2octets(h1)` is `h1 mod n`.
//! 2. Seed `V = 0x01…`, `K = 0x00…`, then apply the RFC's steps d–g with HMAC-SHA-384 over
//!    `V || 0x00 || d || bits2octets(h1)` and `V || 0x01 || d || bits2octets(h1)`.
//! 3. Produce a candidate `T = HMAC_K(V)`; if it is not in `[1, n-1]` apply the retry update
//!    and repeat. Compare, never reduce, so `k` stays uniform.
//! 4. Compute `R = [k]G` and `r = x_R mod n`; if `r = 0`, retry with a new `k`.
//! 5. Compute `s = k^-1 (e + r d) mod n`; if `s = 0`, retry with a new `k`.
//!
//! # Verification walkthrough
//!
//! 1. Reject the signature unless `1 <= r <= n-1` and `1 <= s <= n-1`.
//! 2. Hash the message with SHA-384 and read the digest as a big-endian integer `e`.
//! 3. Compute `w = s^-1 mod n`, `u1 = e w mod n`, `u2 = r w mod n`.
//! 4. Compute `R = [u1]G + [u2]Q`; reject if `R` is the point at infinity.
//! 5. Accept exactly when `x_R mod n == r`.
//!
//! # Published worked example
//!
//! RFC 6979 A.2.6 publishes a private key and the deterministic SHA-384 signature of
//! `"sample"`. Signing reproduces it exactly, and verification accepts it:
//!
//! ```
//! use rsl_crypto::signature::ecdsa_p384::EcdsaP384SigningKey;
//!
//! fn decode<const N: usize>(hex: &str) -> [u8; N] {
//!     core::array::from_fn(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
//! }
//!
//! let key = EcdsaP384SigningKey::from_bytes(decode(concat!(
//!     "6b9d3dad2e1b8c1c05b19875b6659f4de23c3b667bf297ba",
//!     "9aa47740787137d896d5724e4c70a825f872c9ea60d2edf5",
//! )))?;
//! let signature = key.sign_sha384(b"sample")?;
//!
//! let mut expected = [0_u8; 96];
//! expected[..48].copy_from_slice(&decode::<48>(concat!(
//!     "94edbb92a5ecb8aad4736e56c691916b3f88140666ce9fa7",
//!     "3d64c4ea95ad133c81a648152e44acf96e36dd1e80fabe46",
//! )));
//! expected[48..].copy_from_slice(&decode::<48>(concat!(
//!     "99ef4aeb15f178cea1fe40db2603138f130e740a19624526",
//!     "203b6351d0a3a94fa329c145786e679e7b82c71a38628ac8",
//! )));
//! assert_eq!(signature.as_bytes(), &expected);
//!
//! key.verifying_key().verify_sha384(b"sample", &signature)?;
//! # Ok::<(), rsl_crypto::CryptoError>(())
//! ```
//!
//! # Common mistakes and non-goals
//!
//! - Do not accept `r = 0` or `s = 0`; both permit trivial forgeries.
//! - Do not compare `x_R` with `r` before reducing modulo `n`; `x_R` is a field element.
//! - Do not reduce a nonce candidate modulo `n`; RFC 6979 compares and retries to avoid bias.
//! - This API takes raw `r || s`. TLS and X.509 carry ECDSA signatures as DER
//!   `ECDSA-Sig-Value`; that ASN.1 parsing belongs to the protocol or certificate layer.
//! - Randomized `k`, low-`s` normalization, public-key recovery, other hashes, and other curves
//!   are outside this profile.
//!
//! # Readable source map
//!
//! - [`crate::curve::p384`] owns the field, scalar, and point arithmetic.
//! - [`api`] owns typed keys and signatures and the generic
//!   [`Signer`](crate::signature::Signer) and [`Verifier`](crate::signature::Verifier)
//!   contracts.
//! - [`nonce`] owns the RFC 6979 §3.2 generator with its lettered steps.
//! - [`sign`] owns the FIPS 186-5 §6.4.1 signing equation and retry loop.
//! - [`verify`] owns the FIPS 186-5 §6.4.2 step sequence.
//!
//! # Evidence and security status
//!
//! Signing evidence: RFC 6979 A.2.6's published `k` values and both SHA-384 signatures are
//! reproduced exactly, and all 15 NIST CAVP `SigGen` P-384/SHA-384 cases reproduce `(r, s)`
//! from their published `d` and `k`. Verification evidence: all 15 CAVP `SigVer` verdicts, the
//! CAVP `SigGen` signatures, range boundaries for `r` and `s`, and tampering rejection.
//! Development-only differential tests show byte-identical signatures with the `p256` crate
//! 0.14.0 over 32 cases and mutual acceptance. Passing those is not an audit.

#![allow(rustdoc::private_intra_doc_links)]

mod api;
#[cfg(test)]
mod cavp_siggen_fixtures;
mod nonce;
mod sign;
mod verify;

pub use api::{EcdsaP384Signature, EcdsaP384SigningKey, EcdsaP384VerifyingKey};

/// Current project lifecycle classification for ECDSA P-384 with SHA-384.
pub const SECURITY_STATUS: crate::security::SecurityStatus =
    crate::security::SecurityStatus::Recommended;
