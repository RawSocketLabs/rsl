//! ECDSA verification over P-256 with SHA-256, taught from a digest to a checked point.
//!
//! # What ECDSA P-256 verification does
//!
//! A signer with private scalar `d` and public point `Q = [d]G` produces `(r, s)` over the
//! digest of a message. A verifier reconstructs a point from the signature and the digest and
//! accepts exactly when that point's `x`-coordinate, reduced modulo `n`, equals `r`. FIPS 186-5
//! §6.4.2 defines the verification steps; SP 800-186 supplies the curve. TLS 1.3 names this
//! profile `ecdsa_secp256r1_sha256`; SSH names it `ecdsa-sha2-nistp256`.
//!
//! Only verification is implemented here. Signing requires per-signature secret nonces whose
//! generation (FIPS 186-5 §6.3, or RFC 6979 deterministic derivation) is a distinct profile.
//!
//! # Inputs, output, and checked behavior
//!
//! - [`EcdsaP256VerifyingKey`] owns a fully validated 65-byte uncompressed point.
//! - [`EcdsaP256Signature`] owns the fixed 64-byte encoding `r || s`, both big-endian. Range
//!   checks happen at verification so a detached wire field can be parsed on its own.
//! - Verification returns [`CryptoError::InvalidSignature`](crate::CryptoError::InvalidSignature) for `r` or `s` outside `[1, n-1]`,
//!   for a reconstructed point at infinity, and for a failed comparison.
//!
//! # Standards notation in Rust
//!
//! | FIPS 186-5 name | Rust representation | Meaning |
//! | --- | --- | --- |
//! | `Q` | [`EcdsaP256VerifyingKey`] | The signer's public point. |
//! | `(r, s)` | [`EcdsaP256Signature`] | The signature, two scalars in `[1, n-1]`. |
//! | `H(M)`, `e` | [`Sha256Digest`](crate::digest::sha2::sha256::Sha256Digest) then `Scalar::reduce_bytes` | The 256-bit digest as an integer modulo `n`. |
//! | `w = s^-1 mod n` | `Scalar::invert` | Inverse of `s`. |
//! | `u1 = e w`, `u2 = r w` | `Scalar::multiply` | Verification multipliers. |
//! | `R = [u1]G + [u2]Q` | `ProjectivePoint::multiply` and `add` | The reconstructed point. |
//! | `v = x_R mod n` | `Scalar::reduce_limbs` | The candidate for `r`. |
//!
//! # Algorithm walkthrough
//!
//! 1. Reject the signature unless `1 <= r <= n-1` and `1 <= s <= n-1`.
//! 2. Hash the message with SHA-256; because the digest is 256 bits and `n` is 256 bits, the
//!    leftmost-bits rule keeps the whole digest. Read it as a big-endian integer `e`.
//! 3. Compute `w = s^-1 mod n`, `u1 = e w mod n`, `u2 = r w mod n`.
//! 4. Compute `R = [u1]G + [u2]Q`; reject if `R` is the point at infinity.
//! 5. Accept exactly when `x_R mod n == r`.
//!
//! # Published worked example
//!
//! RFC 6979 A.2.5 signs the message `"sample"` with SHA-256 under a published key:
//!
//! ```
//! use rsl_crypto::signature::ecdsa_p256::{EcdsaP256Signature, EcdsaP256VerifyingKey};
//!
//! fn decode<const N: usize>(hex: &str) -> [u8; N] {
//!     core::array::from_fn(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
//! }
//!
//! let mut encoded_key = [0_u8; 65];
//! encoded_key[0] = 0x04;
//! encoded_key[1..33].copy_from_slice(&decode::<32>(
//!     "60FED4BA255A9D31C961EB74C6356D68C049B8923B61FA6CE669622E60F29FB6",
//! ));
//! encoded_key[33..].copy_from_slice(&decode::<32>(
//!     "7903FE1008B8BC99A41AE9E95628BC64F2F1B20C2D7E9F5177A3C294D4462299",
//! ));
//! let key = EcdsaP256VerifyingKey::from_bytes(encoded_key)?;
//!
//! let mut signature = [0_u8; 64];
//! signature[..32].copy_from_slice(&decode::<32>(
//!     "EFD48B2AACB6A8FD1140DD9CD45E81D69D2C877B56AAF991C34D0EA84EAF3716",
//! ));
//! signature[32..].copy_from_slice(&decode::<32>(
//!     "F7CB1C942D657C41D436C7A1B6E29F65F3E900DBB9AFF4064DC4AB2F843ACDA8",
//! ));
//!
//! key.verify_sha256(b"sample", &EcdsaP256Signature::from_bytes(signature))?;
//! # Ok::<(), rsl_crypto::CryptoError>(())
//! ```
//!
//! # Common mistakes and non-goals
//!
//! - Do not accept `r = 0` or `s = 0`; both permit trivial forgeries.
//! - Do not compare `x_R` with `r` before reducing modulo `n`; `x_R` is a field element.
//! - This API takes raw `r || s`. TLS and X.509 carry ECDSA signatures as DER
//!   `ECDSA-Sig-Value`; that ASN.1 parsing belongs to the protocol or certificate layer.
//! - Signing, RFC 6979 nonce derivation, low-`s` normalization, public-key recovery, other
//!   hashes, and other curves are outside this profile.
//!
//! # Readable source map
//!
//! - [`crate::curve::p256`] owns the field, scalar, and point arithmetic.
//! - [`api`] owns typed keys and signatures and the generic
//!   [`Verifier`](crate::signature::Verifier) contract.
//! - [`verify`] owns the FIPS 186-5 §6.4.2 step sequence.
//!
//! # Evidence and security status
//!
//! Public tests cover RFC 6979 A.2.5's two SHA-256 signatures, all 15 NIST CAVP `SigVer`
//! P-256/SHA-256 cases (three accepts and twelve labeled rejections), range boundaries for `r`
//! and `s`, and development-only differential comparison with signatures from the `p256` crate
//! 0.14.0. Passing those is not an audit.

#![allow(rustdoc::private_intra_doc_links)]

mod api;
mod verify;

pub use api::{EcdsaP256Signature, EcdsaP256VerifyingKey};

/// Current project lifecycle classification for ECDSA P-256 with SHA-256.
pub const SECURITY_STATUS: crate::security::SecurityStatus =
    crate::security::SecurityStatus::Recommended;
