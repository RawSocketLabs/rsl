//! HMAC-SHA-256, keyed authentication built from SHA-256.
//!
//! # What HMAC adds to a digest
//!
//! SHA-256 alone is public and unkeyed: anyone can calculate a new digest after changing a
//! message. HMAC adds a shared secret key, producing a tag that only a key holder should be able to
//! reproduce. It protects authenticity and integrity, but it does not hide the message.
//!
//! [NIST FIPS 198-1 §2.3–§4][fips-198-1] defines HMAC. [NIST FIPS 180-4][fips-180-4]
//! supplies SHA-256, and [RFC 4231 §4][rfc-4231] publishes interoperable HMAC-SHA-256 examples.
//!
//! # Notation and parameters
//!
//! | HMAC concept | HMAC-SHA-256 value | Rust representation |
//! | --- | --- | --- |
//! | digest `H` | SHA-256 | [`Sha256`](crate::digest::sha2::sha256::Sha256) |
//! | digest output length | 32 bytes | [`HmacSha256Tag`] |
//! | digest input block `B` | 64 bytes | private normalized-key arrays |
//! | inner pad `ipad` | `0x36` repeated | explicit XOR in `key.rs` |
//! | outer pad `opad` | `0x5c` repeated | explicit XOR in `key.rs` |
//!
//! # Algorithm walkthrough
//!
//! 1. Normalize `K` to one 64-byte block `K0`: zero-pad a short key, retain a 64-byte key, or
//!    hash a longer key and zero-pad the 32-byte digest.
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
//! use rsl_crypto::mac::hmac::sha256::HmacSha256;
//!
//! let tag = HmacSha256::authenticate(&[0x0b; 20], b"Hi There")?;
//! assert_eq!(
//!     tag.into_bytes(),
//!     [
//!         0xb0, 0x34, 0x4c, 0x61, 0xd8, 0xdb, 0x38, 0x53,
//!         0x5c, 0xa8, 0xaf, 0xce, 0xaf, 0x0b, 0xf1, 0x2b,
//!         0x88, 0x1d, 0xc2, 0x00, 0xc9, 0x83, 0x3d, 0xa7,
//!         0x26, 0xe9, 0x37, 0x6c, 0x2e, 0x32, 0xcf, 0xf7,
//!     ]
//! );
//! # Ok::<(), rsl_crypto::CryptoError>(())
//! ```
//!
//! # Incremental verification
//!
//! ```
//! use rsl_crypto::mac::hmac::sha256::HmacSha256;
//!
//! let key = b"example shared secret";
//! let expected = HmacSha256::authenticate(key, b"header || body")?;
//! let mut verifier = HmacSha256::new(key)?;
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
//! - This profile exposes the complete 32-byte tag. A protocol may truncate HMAC only when its
//!   standard specifies the length and associated security policy.
//! - Verification needs the exact bytes and exact framing used by the sender. Encode protocol
//!   structures canonically before authenticating them.
//! - Source-level full-byte comparison has not yet been verified as constant-time for every
//!   compiler and target.
//!
//! # Readable source map
//!
//! `key.rs` owns key normalization and pad XOR. `state.rs` owns the two seeded SHA-256 states,
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

pub use state::{HmacSha256, HmacSha256Tag};

/// Current project lifecycle classification for full-tag HMAC-SHA-256.
pub const SECURITY_STATUS: crate::security::SecurityStatus =
    crate::security::SecurityStatus::Recommended;
