//! SHA-256, from message bytes to a 256-bit digest.
//!
//! # What SHA-256 does
//!
//! SHA-256 is an unkeyed, deterministic digest function standardized by
//! [NIST FIPS 180-4][fips-180-4]. It accepts a byte string of practically arbitrary length and
//! returns exactly 32 bytes. A one-byte input change is intended to unpredictably change the
//! digest, while recovering the input or finding two inputs with the same digest should be
//! computationally infeasible.
//!
//! A digest is **not encryption**: it cannot be decrypted. It is also **not authentication** when
//! an attacker can replace both a message and its digest. Use
//! [`HmacSha256`](crate::mac::hmac::sha256::HmacSha256) when a secret key must authenticate the
//! message.
//!
//! # Standard notation in this implementation
//!
//! | FIPS 180-4 concept | Rust representation |
//! | --- | --- |
//! | 32-bit word | `u32` |
//! | 512-bit message block | `[u8; 64]` |
//! | eight-word chaining value | `[u32; 8]` |
//! | addition modulo `2^32` | [`u32::wrapping_add`] |
//! | final 256-bit digest | [`Sha256Digest`] |
//!
//! Input and output words use big-endian byte order. Bit rotations and logical shifts remain
//! separately named in the source so the standard's equations stay recognizable.
//!
//! # Algorithm walkthrough
//!
//! 1. **Pad:** append one `1` bit, enough zero bits to leave 64 final bits, then the original bit
//!    length. Byte-aligned input represents the first padding bit as `0x80`.
//! 2. **Parse:** split every 64-byte block into sixteen big-endian 32-bit words.
//! 3. **Schedule:** expand those sixteen words into `W[0]` through `W[63]` with the small-sigma
//!    recurrence.
//! 4. **Compress:** perform 64 rounds using `Ch`, `Maj`, the large-sigma functions, one schedule
//!    word, and one round constant.
//! 5. **Feed forward:** add the eight working words back into the chaining value modulo `2^32`.
//! 6. **Serialize:** write the final eight chaining words in big-endian order.
//!
//! For the three-byte message `abc`, padding creates one 64-byte block beginning
//! `61 62 63 80`, followed by zeroes, and ending in the bit length `00 00 00 00 00 00 00 18`.
//! NIST publishes the resulting digest used in the example below.
//!
//! # One-shot example
//!
//! ```
//! use rsl_crypto::digest::sha2::sha256::Sha256;
//!
//! let digest = Sha256::digest(b"abc").expect("three bytes fit SHA-256");
//! assert_eq!(
//!     digest.into_bytes(),
//!     [
//!         0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea,
//!         0x41, 0x41, 0x40, 0xde, 0x5d, 0xae, 0x22, 0x23,
//!         0xb0, 0x03, 0x61, 0xa3, 0x96, 0x17, 0x7a, 0x9c,
//!         0xb4, 0x10, 0xff, 0x61, 0xf2, 0x00, 0x15, 0xad,
//!     ]
//! );
//! ```
//!
//! # Incremental input and text
//!
//! Fragment boundaries do not become part of the message. Text is hashed as its existing UTF-8
//! bytes; this API does not invent a serialization for arbitrary Rust values.
//!
//! ```
//! use rsl_crypto::digest::sha2::sha256::Sha256;
//!
//! let mut state = Sha256::new();
//! state.update("learn ").expect("the fragment is short");
//! state.update(String::from("SHA-256")).expect("the fragment is short");
//! let incremental = state.finalize();
//! let one_shot = Sha256::digest("learn SHA-256").expect("the message is short");
//!
//! assert_eq!(incremental, one_shot);
//! ```
//!
//! # Common mistakes
//!
//! - Do not use a bare digest as a password hash; password storage requires a password-specific,
//!   deliberately expensive construction with salt and policy.
//! - Do not treat `hash(secret || message)` as a substitute for HMAC.
//! - Do not hash an in-memory Rust struct and assume the result is a stable wire digest. Encode a
//!   canonical byte representation first.
//! - A digest match proves byte equality relative to a trusted expected digest, not authorship.
//!
//! # Readable source map
//!
//! The private layers follow the walkthrough: `constants.rs`, `functions.rs`, `schedule.rs`,
//! `compression.rs`, then `state.rs`. The repository builds private-item rustdoc on docs.rs so
//! those formulas and state transitions can be read beside the public guide.
//!
//! # Evidence and security status
//!
//! Tests cover private equations and intermediate state, NIST's published examples, CAVP padding
//! boundaries, arbitrary fragmentation, and differential comparison with a development-only
//! implementation. This is strong implementation evidence, not an independent audit or formal
//! validation. The crate-level [security status](crate#security-status) still applies.
//!
//! [fips-180-4]: https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.180-4.pdf

mod compression;
mod constants;
mod functions;
mod schedule;
mod state;

pub use state::{Sha256, Sha256Digest};

/// Current project lifecycle classification for SHA-256.
pub const SECURITY_STATUS: crate::security::SecurityStatus =
    crate::security::SecurityStatus::Recommended;
