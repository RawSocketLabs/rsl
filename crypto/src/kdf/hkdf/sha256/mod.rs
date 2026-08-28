//! HKDF-SHA-256, extract-then-expand key derivation.
//!
//! # What HKDF does
//!
//! HKDF turns existing input keying material into uniformly useful, purpose-separated output key
//! material. [RFC 5869][rfc-5869] deliberately defines two stages:
//!
//! - **Extract** accepts possibly nonuniform input keying material (`IKM`) and an optional,
//!   non-secret salt, then returns a fixed 32-byte pseudorandom key (`PRK`).
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
//! | `Hash` | SHA-256 inside HMAC | [`HmacSha256`](crate::mac::hmac::sha256::HmacSha256) |
//! | `HashLen` | 32 bytes | [`HkdfSha256Prk::LEN`] |
//! | `salt` | optional extraction salt | `Option<&[u8]>` |
//! | `IKM` | secret input keying material | `&[u8]` |
//! | `PRK` | extracted pseudorandom key | [`HkdfSha256Prk`] |
//! | `info` | application/context binding | `&[u8]` |
//! | `OKM` | caller-owned derived output | `&mut [u8]` |
//!
//! # Algorithm walkthrough
//!
//! 1. If salt is absent, substitute 32 zero bytes. An explicitly empty salt is still an empty
//!    HMAC key; for HMAC these two inputs yield the same result after key normalization.
//! 2. Calculate `PRK = HMAC-SHA-256(salt, IKM)`.
//! 3. Set `T(0)` to empty.
//! 4. For counter values `1..=N`, calculate
//!    `T(i) = HMAC-SHA-256(PRK, T(i-1) || info || i)`.
//! 5. Concatenate the `T(i)` blocks and keep exactly the requested number of bytes.
//!
//! The one-byte counter permits at most 255 SHA-256 output blocks: 8,160 bytes. This
//! implementation rejects a larger request before modifying the caller's output.
//!
//! # Published worked example
//!
//! This is RFC 5869 Appendix A.1. Keeping Extract explicit makes the intermediate `PRK` visible
//! for learning and for protocol key schedules that reuse it.
//!
//! ```
//! use rsl_crypto::kdf::hkdf::sha256::extract;
//!
//! let ikm = [0x0b; 22];
//! let salt = [0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06,
//!             0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c];
//! let info = [0xf0, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6,
//!             0xf7, 0xf8, 0xf9];
//! let prk = extract(Some(&salt), &ikm)?;
//! assert_eq!(
//!     prk.expose_secret(),
//!     &[
//!         0x07, 0x77, 0x09, 0x36, 0x2c, 0x2e, 0x32, 0xdf,
//!         0x0d, 0xdc, 0x3f, 0x0d, 0xc4, 0x7b, 0xba, 0x63,
//!         0x90, 0xb6, 0xc7, 0x3b, 0xb5, 0x0f, 0x9c, 0x31,
//!         0x22, 0xec, 0x84, 0x4a, 0xd7, 0xc2, 0xb3, 0xe5,
//!     ]
//! );
//!
//! let mut okm = [0_u8; 42];
//! prk.expand(&info, &mut okm)?;
//! assert_eq!(
//!     okm,
//!     [
//!         0x3c, 0xb2, 0x5f, 0x25, 0xfa, 0xac, 0xd5, 0x7a,
//!         0x90, 0x43, 0x4f, 0x64, 0xd0, 0x36, 0x2f, 0x2a,
//!         0x2d, 0x2d, 0x0a, 0x90, 0xcf, 0x1a, 0x5a, 0x4c,
//!         0x5d, 0xb0, 0x2d, 0x56, 0xec, 0xc4, 0xc5, 0xbf,
//!         0x34, 0x00, 0x72, 0x08, 0xd5, 0xb8, 0x87, 0x18,
//!         0x58, 0x65,
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
//! use rsl_crypto::kdf::hkdf::sha256::derive;
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
//! Tests cover all three RFC 5869 SHA-256 cases, the exact 8,160-byte limit, atomic rejection,
//! stage composition, recurrence boundaries, and differential comparison. This is implementation
//! evidence, not an audit. The crate-level [security status](crate#security-status) applies.
//!
//! [rfc-5869]: https://www.rfc-editor.org/rfc/rfc5869.html

mod expand;
mod extract;

pub use extract::{HkdfSha256Prk, extract};

/// Current project lifecycle classification for HKDF-SHA-256.
pub const SECURITY_STATUS: crate::security::SecurityStatus =
    crate::security::SecurityStatus::Recommended;

/// SHA-256's output length, called `HashLen` by RFC 5869.
const HASH_LEN: usize = 32;

use crate::Result;

/// Apply RFC 5869 HKDF-Extract followed by HKDF-Expand.
///
/// The named [`extract()`] operation and [`HkdfSha256Prk::expand`] method remain available so the
/// two-stage construction is never hidden from callers that need its intermediate PRK boundary.
/// This convenience function retains neither the borrowed salt nor input keying material.
///
/// # Errors
///
/// Returns [`crate::CryptoError::MessageTooLong`] if input keying material or context exceeds an
/// underlying HMAC-SHA-256 bound, or [`crate::CryptoError::OutputTooLong`] if `output` exceeds
/// 8,160 bytes.
///
/// # Examples
///
/// ```
/// use rsl_crypto::kdf::hkdf::sha256::derive;
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
