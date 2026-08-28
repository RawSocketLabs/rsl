//! Accuracy-first cryptography you can read from standard to source.
//!
//! `rsl-crypto` is both a library and a teaching reference. Its implementations favor a direct
//! correspondence between published notation, named Rust types, and small testable operations.
//! Speed is secondary to making every byte transformation explainable.
//!
//! # Security status
//!
//! > **This is not yet an audited production cryptography library.**
//!
//! The implemented algorithms have published known-answer tests, boundary and negative tests,
//! and development-only differential comparisons. They have not received an independent
//! cryptographic audit, formal NIST validation, or complete compiler-output and platform-level
//! side-channel analysis. The documentation says where source code avoids obvious
//! secret-dependent control flow, but that is not a constant-time guarantee.
//!
//! # Learning path
//!
//! The public implementations form a dependency ladder:
//!
//! 1. [`Sha256`](digest::sha2::sha256::Sha256) turns any byte string into a fixed 32-byte digest.
//!    Begin here to learn padding, message schedules, compression, and incremental input.
//! 2. [`HmacSha256`](mac::hmac::sha256::HmacSha256) combines a secret key with SHA-256 to
//!    authenticate bytes. It demonstrates why an ordinary digest is not a MAC.
//! 3. [`HkdfSha256Prk`](kdf::hkdf::sha256::HkdfSha256Prk) makes RFC 5869's Extract and Expand
//!    stages visible. It demonstrates domain-separated key derivation.
//! 4. [`X25519`](agreement::x25519::X25519) combines private random bytes with a peer's public
//!    coordinate to establish a shared secret. It demonstrates finite-field arithmetic and the
//!    fixed-structure Montgomery ladder without claiming peer authentication.
//! 5. [`Sha512`](digest::sha2::sha512::Sha512) extends the SHA-2 model to 64-bit words and is the
//!    exact digest prerequisite used by Ed25519.
//! 6. [`Ed25519SigningKey`](signature::ed25519::Ed25519SigningKey) derives an authenticated public
//!    identity and deterministic signatures from a private seed.
//! 7. [`Aes128`](cipher::aes::aes128::Aes128) exposes the raw 128-bit block permutation for
//!    studying AES rounds. It is deliberately not a message-encryption API.
//! 8. [`Aes128Gcm`](aead::gcm::Aes128Gcm) composes AES, counter mode, and GHASH into
//!    authenticated encryption with associated data.
//!
//! Follow the links above for the mental model, standard notation, algorithm steps, worked
//! examples, common mistakes, and exact evidence for each construction.
//!
//! # A quick tour
//!
//! Hashing borrows an existing byte representation. `str` and `String` therefore mean their UTF-8
//! bytes; arbitrary Rust values are not implicitly serialized.
//!
//! ```
//! use rsl_crypto::digest::sha2::sha256::Sha256;
//!
//! let digest = Sha256::digest("hello").expect("a short message fits SHA-256");
//! assert_eq!(digest.as_bytes().len(), 32);
//! ```
//!
//! Authenticated encryption returns ciphertext and a detached tag. Associated data stays visible
//! on the wire but is cryptographically bound to the ciphertext. Keys and one-shot random nonces
//! can be generated from an explicitly selected [`RandomSource`]; a
//! stateful protocol should instead derive each nonce from its own sequence rules.
//!
//! ```
//! use rsl_crypto::{
//!     RandomSource, Result,
//!     aead::gcm::{Aes128Gcm, Aes128GcmKey, Aes128GcmNonce},
//! };
//!
//! fn protect_once(random: &mut impl RandomSource) -> Result<()> {
//!     let algorithm = Aes128Gcm::new(Aes128GcmKey::generate(random)?);
//!     let nonce = Aes128GcmNonce::generate(random)?;
//!     let cleartext_header = b"record header";
//!
//!     let sealed = algorithm.seal(&nonce, cleartext_header, b"protected payload")?;
//!     let plaintext = algorithm.open(
//!         &nonce,
//!         cleartext_header,
//!         sealed.ciphertext(),
//!         sealed.tag(),
//!     )?;
//!
//!     assert_eq!(plaintext, b"protected payload");
//!     Ok(())
//! }
//! ```
//!
//! # Bytes, codecs, and protocols
//!
//! Cryptographic primitives operate on bytes, not semantic protocol fields. A wire codec such as
//! RSL `bitsandbytes` should encode a structure first; the resulting bytes can then be hashed,
//! authenticated, or encrypted. TLS and SSH may share [`Aes128Gcm`](aead::gcm::Aes128Gcm), while
//! their repositories retain record/packet framing, transcript rules, nonce construction,
//! sequence numbers, key schedules, replay policy, and encryption activation state.
//!
//! # Type and lifetime policy
//!
//! Equal-sized values are different types when their meanings differ: an AES key is not an AES
//! block, a SHA-256 digest is not an HMAC tag, and a GCM nonce is not a GCM tag. Secret-bearing
//! owners are redacted from formatting and zeroized on drop. Explicit exposure or ownership
//! transfer makes the remaining lifetime the caller's responsibility; see [`Secret`].
//!
//! # Evidence and standards
//!
//! Every algorithm module links its controlling publication and explains which rules it owns.
//! The repository's `STANDARDS.md` is the section-by-section traceability ledger, and
//! `tests/vectors/` records fixture provenance and conversion policy. Passing this evidence is an
//! implementation milestone, not a certification claim.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

extern crate alloc;

pub mod aead;
pub mod agreement;
pub mod cipher;
pub mod digest;
pub mod error;
pub mod kdf;
pub mod mac;
pub mod random;
pub mod secret;
pub mod security;
pub mod signature;

pub use error::{CryptoError, Result};
pub use random::RandomSource;
pub use secret::{Secret, SecretBytes, SecretVec};
