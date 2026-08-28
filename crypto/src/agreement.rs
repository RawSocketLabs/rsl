//! Public-key agreement contracts.
//!
//! Key agreement combines a local private key with a peer public key to produce a shared secret.
//! A protocol then feeds that secret into a specified key schedule such as HKDF; the raw agreement
//! result should not be used directly as an encryption key.
//!
//! [`x25519`] provides the first concrete agreement algorithm. Its primitive performs only the
//! RFC 7748 scalar multiplication and contributory-behavior check. [`ecdh_p256`] provides the
//! SP 800-56A ECC CDH primitive over NIST P-256 with full public-key validation. TLS and SSH
//! still own ephemeral-key generation, encoded key-share framing, transcript authentication, and
//! the key schedule that consumes the shared secret.
//!
//! # Generic use
//!
//! ```
//! use rsl_crypto::{Result, agreement::{KeyAgreement, x25519::X25519}};
//!
//! fn agree<A: KeyAgreement>(
//!     private_key: &A::PrivateKey,
//!     peer_public_key: &A::PublicKey,
//! ) -> Result<A::SharedSecret> {
//!     A::agree(private_key, peer_public_key)
//! }
//!
//! # use rsl_crypto::agreement::x25519::{X25519PrivateKey, X25519PublicKey};
//! # let private_key = X25519PrivateKey::new([0x42; 32]);
//! # let peer_private_key = X25519PrivateKey::new([0x24; 32]);
//! # let peer_public_key: X25519PublicKey = X25519::public_key(&peer_private_key);
//! let shared_secret = agree::<X25519>(&private_key, &peer_public_key)?;
//! assert_eq!(shared_secret.expose_secret().len(), 32);
//! # Ok::<(), rsl_crypto::CryptoError>(())
//! ```

use crate::Result;

pub mod ecdh_p256;
pub mod x25519;

/// A public-key agreement primitive such as an elliptic-curve Diffie-Hellman function.
///
/// Concrete implementations must keep private keys and shared secrets in secret-bearing owner
/// types, validate peer public keys as required by their standard, and document any all-zero or
/// subgroup rejection rules. See the [`agreement` module](crate::agreement) for generic use.
pub trait KeyAgreement {
    /// Secret private-key material.
    type PrivateKey;

    /// The encoded or structured public key.
    type PublicKey;

    /// The secret agreement result.
    type SharedSecret;

    /// Derive the public key corresponding to `private_key`.
    fn public_key(private_key: &Self::PrivateKey) -> Self::PublicKey;

    /// Combine a private key with a peer public key.
    ///
    /// # Errors
    ///
    /// Returns [`crate::CryptoError::InvalidPublicKey`] when the peer key is malformed or is not
    /// a permitted group member, and an algorithm error for another invalid input.
    fn agree(
        private_key: &Self::PrivateKey,
        peer_public_key: &Self::PublicKey,
    ) -> Result<Self::SharedSecret>;
}
