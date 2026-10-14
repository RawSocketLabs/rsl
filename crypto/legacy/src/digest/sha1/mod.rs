//! SHA-1: historically accurate output with broken collision resistance.
//!
//! > **SHA-1 is broken for collision-resistant use and is not permitted for new protection.**
//!
//! SHA-1 maps bytes to a 20-byte digest using five 32-bit chaining words, a big-endian 64-byte
//! block, an 80-word schedule, and eighty rounds. This implementation exists to reproduce
//! historical protocol values and teach the construction. NIST is transitioning away from SHA-1
//! for every cryptographic-protection application and retains the specification for historical
//! handling.
//!
//! # Algorithm walkthrough
//!
//! 1. Pad with `0x80`, zeroes, and the original bit length as a 64-bit big-endian integer.
//! 2. Parse sixteen big-endian words and expand `W_16..W_79` with XOR and a one-bit rotation.
//! 3. Carry five working words through four 20-round phases with different Boolean functions and
//!    additive constants.
//! 4. Add the final working words back into the chaining value and serialize big-endian.
//!
//! # Example
//!
//! ```
//! use rsl_crypto_legacy::digest::sha1::Sha1;
//!
//! let digest = Sha1::digest("abc")?;
//! assert_eq!(&digest.as_bytes()[..4], &[0xa9, 0x99, 0x3e, 0x36]);
//! # Ok::<(), rsl_crypto_legacy::CryptoError>(())
//! ```
//!
//! # Boundaries and non-goals
//!
//! This is a byte-oriented digest, not HMAC-SHA-1, TLS's historical combined MD5/SHA-1 PRF, a
//! signature algorithm, or collision detection. Protocol repositories own those constructions
//! and must require explicit legacy policy. Passing vectors does not repair SHA-1's security.
//!
//! The controlling algorithm definition is NIST FIPS 180-4 §§4.1.1, 4.2.1, 5.1.1, 5.2.1,
//! 5.3.1, and 6.1. Current lifecycle evidence and exact links are recorded in `STANDARDS.md`.

#![allow(rustdoc::private_intra_doc_links)]

mod compression;
mod state;

pub use state::{Sha1, Sha1Digest};

/// SHA-1's lifecycle status: practical collisions break an intended security property.
pub const SECURITY_STATUS: crate::SecurityStatus = crate::SecurityStatus::Broken;
