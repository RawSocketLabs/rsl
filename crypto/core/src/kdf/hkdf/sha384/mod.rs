//! HKDF-SHA-384, extract-then-expand key derivation.
//!
//! # What HKDF does
//!
//! HKDF turns existing input keying material into uniformly useful, purpose-separated output key
//! material. [RFC 5869][rfc-5869] deliberately defines two stages:
//!
//! - **Extract** accepts possibly nonuniform input keying material (`IKM`) and an optional,
//!   non-secret salt, then returns a fixed 48-byte pseudorandom key (`PRK`).
//! - **Expand** accepts that `PRK`, context information (`info`), and an output length, then
//!   derives the requested output keying material (`OKM`).
//!
//! HKDF does not create entropy, replace password hashing, or negotiate what `info` means. A
//! protocol must specify the encoding and purpose labels it places in `info`.
//!
//! # Notation and types
//!
//! | RFC 5869 term | Meaning | Rust representation |
//! | --- | --- | --- |
//! | `Hash` | SHA-384 inside HMAC | [`HmacSha384`](crate::mac::hmac::sha384::HmacSha384) |
//! | `HashLen` | 48 bytes | [`HkdfSha384Prk::LEN`] |
//! | `salt` | optional extraction salt | `Option<&[u8]>` |
//! | `IKM` | secret input keying material | `&[u8]` |
//! | `PRK` | extracted pseudorandom key | [`HkdfSha384Prk`] |
//! | `info` | application/context binding | `&[u8]` |
//! | `OKM` | caller-owned derived output | `&mut [u8]` |
//!
//! # Algorithm walkthrough
//!
//! 1. If salt is absent, substitute 32 zero bytes. An explicitly empty salt is still an empty
//!    HMAC key; for HMAC these two inputs yield the same result after key normalization.
//! 2. Calculate `PRK = HMAC-SHA-384(salt, IKM)`.
//! 3. Set `T(0)` to empty.
//! 4. For counter values `1..=N`, calculate
//!    `T(i) = HMAC-SHA-384(PRK, T(i-1) || info || i)`.
//! 5. Concatenate the `T(i)` blocks and keep exactly the requested number of bytes.
//!
//! The one-byte counter permits at most 255 SHA-384 output blocks: 12,240 bytes. This
//! implementation rejects a larger request before modifying the caller's output.
//!
//! # Published worked example
//!
//! This is a Project Wycheproof `hkdf_sha384` case (RFC 5869 publishes no SHA-384 vectors).
//! Keeping Extract explicit makes the intermediate `PRK` visible
//! for learning and for protocol key schedules that reuse it.
//!
//! ```
//! use rsl_crypto::kdf::hkdf::sha384::extract;
//!
//! // Project Wycheproof hkdf_sha384_test.json, tcId 12.
//! let ikm = [
//!         0x7a, 0x00, 0x81, 0x76, 0x89, 0xa3, 0xd7, 0x90,
//!         0x01, 0x82, 0x5a, 0x86, 0x4c, 0x69, 0xc1, 0x20,
//! ];
//! let salt = [
//!         0x08, 0xbc, 0x01, 0xc0, 0x53, 0xa6, 0x40, 0x6c,
//!         0x7c, 0x4a, 0x66, 0x7c, 0x9b, 0x9b, 0x38, 0x94,
//! ];
//! let info = [
//!         0x96, 0x7c, 0xcd, 0x75, 0x39, 0x5b, 0xe6, 0xe9,
//!         0x6a, 0x67, 0x75, 0x9f, 0x07, 0x04, 0x87, 0xc9,
//!         0xe2, 0x10, 0x77, 0x91,
//! ];
//! let prk = extract(Some(&salt), &ikm)?;
//! assert_eq!(prk.expose_secret().len(), 48);
//!
//! let mut okm = [0_u8; 64];
//! prk.expand(&info, &mut okm)?;
//! assert_eq!(
//!     okm,
//!     [
//!         0xbd, 0x02, 0xe1, 0x6b, 0x60, 0x24, 0xf2, 0xc3,
//!         0xb7, 0x52, 0xd1, 0xc1, 0xd3, 0x04, 0x75, 0x83,
//!         0x69, 0x77, 0x31, 0x91, 0x5f, 0xbb, 0xb3, 0x44,
//!         0x18, 0xf4, 0x79, 0xb0, 0xc9, 0xbf, 0x84, 0xa8,
//!         0x6b, 0xd8, 0xe7, 0x15, 0xec, 0xa1, 0x98, 0xda,
//!         0x8f, 0x9b, 0x39, 0xb2, 0x5a, 0x12, 0x29, 0xc3,
//!         0x11, 0x85, 0x3f, 0x86, 0x23, 0x40, 0xcd, 0xef,
//!         0xe4, 0x6d, 0xdf, 0x41, 0xdc, 0xf2, 0x56, 0xd9,
//!     ]
//! );
//! # Ok::<(), rsl_crypto::CryptoError>(())
//! ```
//!
//! # Convenience composition
//!
//! [`derive()`] performs Extract followed immediately by Expand when the caller does not need to
//! retain the intermediate `PRK`:
//!
//! ```
//! use rsl_crypto::kdf::hkdf::sha384::derive;
//!
//! let mut encryption_key = [0_u8; 16];
//! derive(
//!     Some(b"public salt"),
//!     b"secret input keying material",
//!     b"example encryption key",
//!     &mut encryption_key,
//! )?;
//! assert_ne!(encryption_key, [0_u8; 16]);
//! # Ok::<(), rsl_crypto::CryptoError>(())
//! ```
//!
//! # Common mistakes
//!
//! - Salt and `info` have different roles. Salt strengthens and separates extraction; `info`
//!   binds expansion to a purpose or context.
//! - Salt and `info` do not need to be secret, but their encodings must be unambiguous and agreed
//!   by both sides.
//! - Reusing the same `PRK` with the same `info` yields the same output. Use distinct protocol-
//!   specified labels for distinct keys, IVs, and directions.
//! - Caller-owned output contains secret key material and must receive an appropriate lifetime and
//!   destruction policy after this function returns.
//! - HKDF is not a password hash and does not make low-entropy passwords suitable as keys.
//!
//! # Readable source map
//!
//! `extract.rs` owns salt handling and the secret `PRK`. `expand.rs` owns the `T(i)` recurrence,
//! counter, exact output slicing, and atomic length rejection. This module owns only their named
//! convenience composition.
//!
//! # Evidence and security status
//!
//! Tests cover all 83 Wycheproof HKDF-SHA-384 cases, the exact 12,240-byte limit, atomic rejection,
//! stage composition, recurrence boundaries, and differential comparison. This is implementation
//! evidence, not an audit. The crate-level [security status](crate#security-status) applies.
//!
//! [rfc-5869]: https://www.rfc-editor.org/rfc/rfc5869.html

mod expand;
mod extract;

pub use extract::{HkdfSha384Prk, extract};

/// Current project lifecycle classification for HKDF-SHA-384.
pub const SECURITY_STATUS: crate::security::SecurityStatus =
    crate::security::SecurityStatus::Recommended;

/// SHA-384's output length, called `HashLen` by RFC 5869.
const HASH_LEN: usize = 48;

use crate::Result;

/// Apply RFC 5869 HKDF-Extract followed by HKDF-Expand.
///
/// The named [`extract()`] operation and [`HkdfSha384Prk::expand`] method remain available so the
/// two-stage construction is never hidden from callers that need its intermediate PRK boundary.
/// This convenience function retains neither the borrowed salt nor input keying material.
///
/// # Errors
///
/// Returns [`crate::CryptoError::MessageTooLong`] if input keying material or context exceeds an
/// underlying HMAC-SHA-384 bound, or [`crate::CryptoError::OutputTooLong`] if `output` exceeds
/// 12,240 bytes.
///
/// # Examples
///
/// ```
/// use rsl_crypto::kdf::hkdf::sha384::derive;
///
/// let mut output = [0_u8; 32];
/// derive(Some(b"salt"), b"input keying material", b"example purpose", &mut output)?;
/// assert_ne!(output, [0_u8; 32]);
/// # Ok::<(), rsl_crypto::CryptoError>(())
/// ```
pub fn derive(
    salt: Option<&[u8]>,
    input_key_material: &[u8],
    info: &[u8],
    output: &mut [u8],
) -> Result<()> {
    extract(salt, input_key_material)?.expand(info, output)
}
