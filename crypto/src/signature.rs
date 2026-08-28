//! Digital-signature contracts.
//!
//! A signature binds a message to a private signing key and is checked with the corresponding
//! public verification key. Signatures provide authenticity, not confidentiality. Protocols must
//! specify the signed byte encoding, algorithm identifiers, key validation, and whether messages
//! are prehashed.
//!
//! [`ed25519`] is the first concrete implementation. It provides deterministic signing and strict
//! verification while keeping message encoding, identity binding, certificate formats, and
//! handshake transcript construction in the consuming protocol. [`ecdsa_p256`] adds FIPS 186-5
//! ECDSA over NIST P-256 with SHA-256: deterministic RFC 6979 signing and verification for
//! certificate and handshake interoperability. [`rsa_pss`] adds RFC 8017 RSASSA-PSS verification
//! with SHA-256 for RSA certificates.

use crate::{Result, random::RandomSource};

pub mod ecdsa_p256;
pub mod ed25519;
pub mod rsa_pss;

/// A private-key signing operation.
pub trait Signer {
    /// The concrete signature representation.
    type Signature;

    /// Sign one complete message using explicit randomness.
    ///
    /// A deterministic signature scheme may ignore `random`; randomized schemes consume exactly
    /// the entropy their specification requires.
    ///
    /// # Errors
    ///
    /// Returns an algorithm error for invalid key material or
    /// [`crate::CryptoError::EntropyUnavailable`] when required randomness is unavailable.
    fn sign<R: RandomSource>(&self, random: &mut R, message: &[u8]) -> Result<Self::Signature>;
}

/// A public-key signature-verification operation.
pub trait Verifier<Signature: ?Sized> {
    /// Verify `signature` over one complete message.
    ///
    /// # Errors
    ///
    /// Returns [`crate::CryptoError::InvalidSignature`] when parsing or verification fails.
    fn verify(&self, message: &[u8], signature: &Signature) -> Result<()>;
}
