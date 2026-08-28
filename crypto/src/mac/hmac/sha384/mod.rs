//! HMAC-SHA-384, keyed authentication built from SHA-384.
//!
//! # What HMAC adds to a digest
//!
//! SHA-384 alone is public and unkeyed: anyone can calculate a new digest after changing a
//! message. HMAC adds a shared secret key, producing a tag that only a key holder should be able to
//! reproduce. It protects authenticity and integrity, but it does not hide the message.
//!
//! [NIST FIPS 198-1 §2.3–§4][fips-198-1] defines HMAC. [NIST FIPS 180-4][fips-180-4]
//! supplies SHA-384, and [RFC 4231 §4][rfc-4231] publishes interoperable HMAC-SHA-384 examples.
//!
//! # Notation and parameters
//!
//! | HMAC concept | HMAC-SHA-384 value | Rust representation |
//! | --- | --- | --- |
//! | digest `H` | SHA-384 | [`Sha384`](crate::digest::sha2::sha384::Sha384) |
//! | digest output length | 48 bytes | [`HmacSha384Tag`] |
//! | digest input block `B` | 128 bytes | private normalized-key arrays |
//! | inner pad `ipad` | `0x36` repeated | explicit XOR in `key.rs` |
//! | outer pad `opad` | `0x5c` repeated | explicit XOR in `key.rs` |
//!
//! # Algorithm walkthrough
//!
//! 1. Normalize `K` to one 128-byte block `K0`: zero-pad a short key, retain a 128-byte key, or
//!    hash a longer key and zero-pad the 48-byte digest.
//! 2. Calculate all bytes of `K0 XOR ipad` and `K0 XOR opad`.
//! 3. Calculate the inner digest `H((K0 XOR ipad) || message)`.
//! 4. Calculate the final tag `H((K0 XOR opad) || inner_digest)`.
//! 5. During verification, compare every full-tag byte before returning one uniform
//!    authentication result.
//!
//! The two nested hashes are essential. Ad-hoc constructions such as `SHA256(key || message)` do
//! not inherit HMAC's specified security analysis.
//!
//! # Published worked example
//!
//! RFC 4231 Test Case 1 uses twenty `0x0b` key bytes and the message `Hi There`:
//!
//! ```
//! use rsl_crypto::mac::hmac::sha384::HmacSha384;
//!
//! let tag = HmacSha384::authenticate(&[0x0b; 20], b"Hi There")?;
//! assert_eq!(
//!     tag.into_bytes(),
//!     [
//!         0xaf, 0xd0, 0x39, 0x44, 0xd8, 0x48, 0x95, 0x62,
//!         0x6b, 0x08, 0x25, 0xf4, 0xab, 0x46, 0x90, 0x7f,
//!         0x15, 0xf9, 0xda, 0xdb, 0xe4, 0x10, 0x1e, 0xc6,
//!         0x82, 0xaa, 0x03, 0x4c, 0x7c, 0xeb, 0xc5, 0x9c,
//!         0xfa, 0xea, 0x9e, 0xa9, 0x07, 0x6e, 0xde, 0x7f,
//!         0x4a, 0xf1, 0x52, 0xe8, 0xb2, 0xfa, 0x9c, 0xb6,
//!     ]
//! );
//! # Ok::<(), rsl_crypto::CryptoError>(())
//! ```
//!
//! # Incremental verification
//!
//! ```
//! use rsl_crypto::mac::hmac::sha384::HmacSha384;
//!
//! let key = b"example shared secret";
//! let expected = HmacSha384::authenticate(key, b"header || body")?;
//! let mut verifier = HmacSha384::new(key)?;
//! verifier.update("header")?;
//! verifier.update(String::from(" || body"))?;
//! verifier.verify(expected.as_bytes())?;
//! # Ok::<(), rsl_crypto::CryptoError>(())
//! ```
//!
//! # Common mistakes
//!
//! - Key acceptance is not key generation. A primitive may accept an empty or weak key, but an
//!   application must provision secret key material with suitable entropy.
//! - The message is authenticated, not encrypted; observers can still read it.
//! - This profile exposes the complete 48-byte tag. A protocol may truncate HMAC only when its
//!   standard specifies the length and associated security policy.
//! - Verification needs the exact bytes and exact framing used by the sender. Encode protocol
//!   structures canonically before authenticating them.
//! - Source-level full-byte comparison has not yet been verified as constant-time for every
//!   compiler and target.
//!
//! # Readable source map
//!
//! `key.rs` owns key normalization and pad XOR. `state.rs` owns the two seeded SHA-384 states,
//! incremental message input, finalization, and verification.
//!
//! # Evidence and security status
//!
//! Tests cover every RFC 4231 case, key-normalization boundaries, fragmentation, wrong tags and
//! lengths, and development-only differential comparison. This is implementation evidence, not
//! an audit or formal validation. The crate-level [security status](crate#security-status) applies.
//!
//! [fips-198-1]: https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.198-1.pdf
//! [fips-180-4]: https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.180-4.pdf
//! [rfc-4231]: https://www.rfc-editor.org/rfc/rfc4231.html

mod key;
mod state;

pub use state::{HmacSha384, HmacSha384Tag};

/// Current project lifecycle classification for full-tag HMAC-SHA-384.
pub const SECURITY_STATUS: crate::security::SecurityStatus =
    crate::security::SecurityStatus::Recommended;
