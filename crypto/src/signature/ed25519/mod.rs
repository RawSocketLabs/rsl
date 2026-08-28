//! Pure Ed25519 signatures, taught from encoded points through verification.
//!
//! # What Ed25519 does
//!
//! Ed25519 deterministically signs an exact byte string with a 32-byte private seed. A verifier
//! uses the corresponding 32-byte public key to check the detached 64-byte signature. Signatures
//! authenticate bytes; they do not encrypt them, assign an identity to a bare key, or decide what
//! a TLS or SSH transcript must contain.
//!
//! # Inputs and outputs
//!
//! - [`Ed25519SigningKey`] owns and clears the private seed.
//! - [`Ed25519VerifyingKey`] contains a canonical, non-small-order Edwards point.
//! - [`Ed25519Signature`] stores `ENC(R) || ENC(S)` exactly as it appears on the wire.
//! - [`Ed25519SigningKey::sign`] implements pure Ed25519: no context and no prehash.
//! - [`Ed25519SigningKey::sign_with_context`] implements Ed25519ctx with a mandatory
//!   [`Ed25519Context`]; [`Ed25519SigningKey::sign_prehashed`] implements Ed25519ph over a
//!   caller-computed SHA-512 digest with an optional context. Each variant has its own
//!   verification method; none is selected by a flag on the pure API.
//! - [`Ed25519VerifyingKey::verify`] uses strict point/scalar parsing and the uncofactored equation.
//!
//! # RFC notation in Rust
//!
//! | RFC 8032 name | Rust layer | Meaning |
//! | --- | --- | --- |
//! | `H` | [`Sha512`](crate::digest::sha2::sha512::Sha512) | SHA-512 used at all three hash boundaries. |
//! | `p`, `d` | [`field`] | Edwards25519 field and curve constant. |
//! | `B`, `A`, `R` | [`point::EdwardsPoint`] | Extended-coordinate points with canonical encoding. |
//! | `L`, `r`, `k`, `S` | [`scalar::Scalar`] | Prime-order scalar residues and canonical `S`. |
//! | private seed | [`Ed25519SigningKey`] | The original 32 random bytes, not an X25519 scalar. |
//! | `ENC(R) || ENC(S)` | [`Ed25519Signature`] | Detached 64-byte signature. |
//! | `dom2(F, C)` | private `api::dom2` | `"SigEd25519 no Ed25519 collisions" \|\| F \|\| len(C) \|\| C`; empty for pure Ed25519. |
//! | `C` | [`Ed25519Context`] | A 1–255 byte public context string. |
//! | `PH(M)` | [`Sha512Digest`](crate::digest::sha2::sha512::Sha512Digest) | The Ed25519ph prehash `SHA-512(M)`. |
//!
//! # Signing walkthrough
//!
//! 1. Hash the seed with SHA-512, prune the first 32 bytes into secret scalar `s`, and retain the
//!    upper 32 bytes as the secret prefix.
//! 2. Derive public key `A = [s]B`.
//! 3. Derive deterministic nonce `r = SHA-512(prefix || message) mod L` and `R = [r]B`.
//! 4. Derive challenge `k = SHA-512(ENC(R) || ENC(A) || message) mod L`.
//! 5. Encode `S = (r + k*s) mod L` beside `R`.
//!
//! Verification canonically decodes `A`, `R`, and `S`, rejects small-order points, recomputes
//! `k`, and checks `[S]B = R + [k]A`.
//!
//! Ed25519ctx prefixes `dom2(0, C)` to both hash inputs in steps 3 and 4; Ed25519ph prefixes
//! `dom2(1, C)` and replaces the message with `PH(M) = SHA-512(M)`. Everything else is shared,
//! which is why one private signing core serves all three variants.
//!
//! # Published example
//!
//! ```
//! use rsl_crypto::signature::ed25519::Ed25519SigningKey;
//!
//! let seed = [
//!     0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60,
//!     0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
//!     0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19,
//!     0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
//! ];
//! let signing_key = Ed25519SigningKey::from_seed(seed);
//! let verifying_key = signing_key.verifying_key();
//! let signature = signing_key.sign(b"")?;
//!
//! assert_eq!(&verifying_key.as_bytes()[..4], &[0xd7, 0x5a, 0x98, 0x01]);
//! verifying_key.verify(b"", &signature)?;
//! # Ok::<(), rsl_crypto::CryptoError>(())
//! ```
//!
//! # Common mistakes and non-goals
//!
//! - X25519 and Ed25519 use related fields but have different encodings and scalar semantics;
//!   their keys are not interchangeable.
//! - Ed25519 signs the exact supplied bytes. Protocol repositories must define and encode the
//!   signed transcript without ambiguity.
//! - Pure Ed25519, Ed25519ctx, and Ed25519ph signatures are not interchangeable even for the
//!   same key and message; a verifier must call the variant the protocol specifies.
//! - Ed25519ph does not make the scheme collision-resilient the way pure Ed25519 is; RFC 8032
//!   §8.5 recommends pure Ed25519 unless streaming or a legacy interface requires prehashing.
//! - Deterministic signing avoids runtime nonce entropy, but seed generation still requires an
//!   approved cryptographic random source.
//! - Fixed source structure is not proof of constant-time compiler output or production safety.
//! - Certificate parsing, SSH key blobs, TLS `CertificateVerify`, and algorithm negotiation stay
//!   in their protocol repositories.
//!
//! # Evidence
//!
//! RFC 8032 §7.1 published vectors cover pure key derivation, signing, and verification; §7.2's
//! four Ed25519ctx vectors and §7.3's Ed25519ph vector cover the variants. Boundary tests cover
//! canonical encodings, scalar range, changed messages/signatures, small-order rejection, context
//! length limits, cross-variant rejection, and wire lengths. Development-only differential cases
//! compare pure signatures against `ed25519-dalek` strict verification and Ed25519ph signatures
//! against its prehashed path. This evidence is not an independent audit.

#![allow(rustdoc::private_intra_doc_links)]

mod api;
mod field;
mod point;
mod scalar;

pub use api::{Ed25519Context, Ed25519Signature, Ed25519SigningKey, Ed25519VerifyingKey};

/// Current project lifecycle classification for pure Ed25519 with strict verification.
pub const SECURITY_STATUS: crate::security::SecurityStatus =
    crate::security::SecurityStatus::Recommended;
