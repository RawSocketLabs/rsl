//! SHA-384, the truncated 64-bit-word SHA-2 member.
//!
//! SHA-384 is defined by FIPS 180-4 §6.5 as SHA-512 with two changes: it starts from its own
//! eight initial words (§5.3.4) and outputs only the leftmost 384 bits of the final state. The
//! message schedule, eighty rounds, round constants, and 128-byte block padding are exactly
//! SHA-512's, so this module reuses those private layers rather than transcribing them again.
//! TLS 1.3 uses SHA-384 in `TLS_AES_256_GCM_SHA384`, HKDF-SHA-384, and `ecdsa_secp384r1_sha384`.
//!
//! # How to read this implementation
//!
//! 1. [`constants`] transcribes the SHA-384 initial words.
//! 2. [`state`] owns incremental input, reuses SHA-512's padding and compression, and serializes
//!    the first six chaining words. A white-box test checks the two discarded words against
//!    NIST's printed values.
//!
//! # Example
//!
//! ```
//! use rsl_crypto::digest::sha2::sha384::Sha384;
//!
//! let digest = Sha384::digest("abc")?;
//! assert_eq!(digest.as_bytes().len(), 48);
//! assert_eq!(&digest.as_bytes()[..4], &[0xcb, 0x00, 0x75, 0x3f]);
//! # Ok::<(), rsl_crypto::CryptoError>(())
//! ```
//!
//! # Standard and security status
//!
//! The controlling publication is [NIST FIPS 180-4][fips-180-4], §§5.3.4 and 6.5 together with
//! the SHA-512 sections. Truncation to 384 bits does not weaken the compression function; it
//! fixes the output length, and length-extension is not possible because the attacker lacks
//! `H_6` and `H_7`. Published examples, CAVP boundary vectors, and differential tests provide
//! implementation evidence, not a production audit.
//!
//! [fips-180-4]: https://doi.org/10.6028/NIST.FIPS.180-4

#![allow(rustdoc::private_intra_doc_links)]

mod constants;
mod state;

pub use state::{Sha384, Sha384Digest};

/// Current project lifecycle classification for SHA-384.
pub const SECURITY_STATUS: crate::security::SecurityStatus =
    crate::security::SecurityStatus::Recommended;
