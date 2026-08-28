//! SHA-512, from 64-bit words to a 64-byte digest.
//!
//! SHA-512 is the 64-bit-word member of the SHA-2 family. It processes 128-byte blocks through
//! eighty compression rounds and emits all eight final 64-bit chaining words. Ed25519 uses this
//! exact digest during key expansion, deterministic nonce derivation, and challenge derivation.
//!
//! # How to read this implementation
//!
//! 1. [`constants`] transcribes the initial words and eighty round constants.
//! 2. [`functions`] names the six bit functions from FIPS 180-4 §4.1.3.
//! 3. [`schedule`] parses sixteen big-endian words and expands them to eighty.
//! 4. [`compression`] follows the eighty-round state transition and feed-forward.
//! 5. [`state`] owns incremental input, SHA-512 padding, and digest serialization.
//!
//! # Example
//!
//! ```
//! use rsl_crypto::digest::sha2::sha512::Sha512;
//!
//! let digest = Sha512::digest("abc")?;
//! assert_eq!(digest.as_bytes().len(), 64);
//! assert_eq!(&digest.as_bytes()[..4], &[0xdd, 0xaf, 0x35, 0xa1]);
//! # Ok::<(), rsl_crypto::CryptoError>(())
//! ```
//!
//! # Standard and security status
//!
//! The controlling publication is [NIST FIPS 180-4][fips-180-4], especially §§3.1–3.2,
//! 4.1.3, 4.2.3, 5.1.2, 5.2.2, 5.3.5, and 6.4. Published examples and differential tests provide
//! implementation evidence, not a production audit or a collision-resistance guarantee for an
//! arbitrary higher-level construction.
//!
//! [fips-180-4]: https://doi.org/10.6028/NIST.FIPS.180-4

// This teaching page intentionally links into private executable-specification layers, which
// docs.rs and the documented local command render with `--document-private-items`.
#![allow(rustdoc::private_intra_doc_links)]

mod compression;
mod constants;
mod functions;
mod schedule;
mod state;

pub use state::{Sha512, Sha512Digest};

/// Current project lifecycle classification for SHA-512.
pub const SECURITY_STATUS: crate::security::SecurityStatus =
    crate::security::SecurityStatus::Recommended;
