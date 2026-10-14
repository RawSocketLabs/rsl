//! Historical and broken cryptography behind an explicit dependency boundary.
//!
//! > **Do not use this package to design new protection.**
//!
//! `rsl-crypto-legacy` exists for decoding historical traffic, controlled interoperability labs,
//! test fixtures, and teaching why obsolete constructions failed. It is a separate package so
//! enabling contemporary `rsl-crypto` never makes these algorithms available accidentally.
//!
//! # Non-negotiation rule
//!
//! A primitive in this package performs only its named mathematical transformation. TLS, SSH,
//! and other protocol repositories must require an explicit legacy allowlist and must never
//! silently fall back to one of these algorithms. Successful output proves implementation
//! compatibility, not confidentiality, integrity, authentication, or present-day suitability.
//!
//! # Planned learning path
//!
//! 1. SHA-1 and MD5 show historical Merkle–Damgård digests and their collision failures.
//! 2. RC4 shows a compact stream cipher and the consequences of biased keystream output.
//! 3. DES and Triple-DES show a Feistel network, short keys, and small block-size limits.
//! 4. CBC-era primitives provide only the transformations needed by explicitly selected protocol
//!    profiles; record padding and MAC ordering remain protocol-owned.
//! 5. RSA PKCS #1 v1.5 provides historical signature/encryption encoding with its oracle hazards.
//!
//! Each implementation receives its own teaching page, published vectors, negative cases,
//! differential evidence, and current security references before being advertised as functional.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

extern crate alloc;

pub mod cipher;
pub mod digest;
pub mod rsa;

pub use rsl_crypto::security::{SecurityClassification, SecurityStatus};
pub use rsl_crypto::{CryptoError, Result, Secret, SecretBytes, SecretVec, random::RandomSource};

/// The strongest lifecycle class permitted anywhere in this package.
///
/// Individual algorithms may be classified more severely as [`SecurityStatus::Broken`] or
/// [`SecurityStatus::EducationalOnly`].
pub const PACKAGE_SECURITY_FLOOR: SecurityStatus = SecurityStatus::Legacy;
