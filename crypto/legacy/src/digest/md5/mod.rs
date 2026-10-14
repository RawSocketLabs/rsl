//! MD5: exact RFC 1321 behavior with practical collision attacks.
//!
//! > **MD5 is cryptographically broken and must not protect new data.**
//!
//! MD5 produces a 16-byte digest from four 32-bit chaining words. Unlike SHA-family encodings,
//! MD5 parses words, appends its message length, and serializes its digest in little-endian order.
//! RFC 6151 replaces RFC 1321's original security claims and says MD5 is unacceptable whenever
//! collision resistance is required.
//!
//! # Algorithm walkthrough
//!
//! 1. Append `0x80`, zeroes to byte 56 modulo 64, and the original bit length modulo `2^64` in
//!    little-endian order.
//! 2. Parse sixteen little-endian words.
//! 3. Execute four 16-step rounds using RFC 1321's `F`, `G`, `H`, and `I` functions, message-index
//!    permutations, additive sine-derived constants, and rotation schedule.
//! 4. Feed the four working words forward and serialize them little-endian.
//!
//! # Example
//!
//! ```
//! use rsl_crypto_legacy::digest::md5::Md5;
//!
//! let digest = Md5::digest("abc")?;
//! assert_eq!(digest.as_bytes(), &[
//!     0x90, 0x01, 0x50, 0x98, 0x3c, 0xd2, 0x4f, 0xb0,
//!     0xd6, 0x96, 0x3f, 0x7d, 0x28, 0xe1, 0x7f, 0x72,
//! ]);
//! # Ok::<(), rsl_crypto_legacy::CryptoError>(())
//! ```
//!
//! This module implements only the digest. HMAC-MD5, TLS's historical PRF, certificate
//! signatures, and challenge-response constructions require separately named profiles and
//! protocol policy. Correct digest output does not restore collision resistance.

#![allow(rustdoc::private_intra_doc_links)]

mod compression;
mod state;

pub use state::{Md5, Md5Digest};

/// MD5's lifecycle status: practical collisions break an intended security property.
pub const SECURITY_STATUS: crate::SecurityStatus = crate::SecurityStatus::Broken;
