//! Typed public boundary for X25519 key agreement.
//!
//! ## Standards ownership
//!
//! [RFC 7748 §6.1][rfc-7748] derives a public u-coordinate by applying X25519 to the base
//! coordinate nine, then applies the same function to a private scalar and the peer coordinate.
//! It permits detecting small-order input through an OR of every output byte. This API always
//! performs that check and returns [`CryptoError::InvalidPublicKey`] instead of constructing an
//! all-zero shared-secret owner.
//!
//! Exact key-share encoding, ephemeral-key generation, public-key inclusion in a KDF, transcript
//! authentication, and abort behavior belong to the consuming TLS, SSH, or other protocol.
//!
//! [rfc-7748]: https://www.rfc-editor.org/rfc/rfc7748.html

use core::fmt;
use zeroize::Zeroize;

use super::ladder::scalar_multiply;
use crate::{CryptoError, Result, SecretBytes, agreement::KeyAgreement};

/// Number of bytes in every X25519 scalar, coordinate, and result.
const X25519_BYTES: usize = 32;

/// RFC 7748 §4.1's Curve25519 base u-coordinate, encoded little-endian.
const BASE_COORDINATE: [u8; X25519_BYTES] = [
    9, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// One owned 32-byte X25519 private scalar input.
///
/// These bytes are stored as generated; RFC 7748 scalar preparation occurs inside every X25519
/// operation. The owner is non-`Clone`, redacted, and zeroized on drop. Random generation is
/// deliberately external because the protocol context must choose an approved entropy source and
/// key lifetime.
///
/// # Examples
///
/// ```
/// use rsl_crypto::agreement::x25519::{X25519, X25519PrivateKey};
///
/// let private_key = X25519PrivateKey::new([0x42; 32]);
/// let public_key = X25519::public_key(&private_key);
/// assert_eq!(public_key.as_bytes().len(), X25519PrivateKey::LEN);
/// assert_eq!(format!("{private_key:?}"), "X25519PrivateKey([REDACTED])");
/// ```
pub struct X25519PrivateKey {
    bytes: SecretBytes<X25519_BYTES>,
}

impl X25519PrivateKey {
    /// Size of the private scalar input in bytes.
    pub const LEN: usize = X25519_BYTES;

    /// Take ownership of 32 random bytes for use as an X25519 scalar input.
    #[must_use]
    pub fn new(bytes: [u8; X25519_BYTES]) -> Self {
        Self {
            bytes: SecretBytes::new(bytes),
        }
    }
}

impl fmt::Debug for X25519PrivateKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("X25519PrivateKey([REDACTED])")
    }
}

/// One received or derived 32-byte X25519 Montgomery u-coordinate.
///
/// Public keys are not secret. Construction validates only encoded length because RFC 7748 §5
/// requires X25519 implementations to accept non-canonical field encodings and coordinates on
/// the twist. [`X25519::agree`] rejects the all-zero result that identifies small-order inputs.
///
/// # Examples
///
/// ```
/// use rsl_crypto::agreement::x25519::X25519PublicKey;
///
/// let wire_bytes = [0x24; 32];
/// let public_key = X25519PublicKey::try_from(wire_bytes.as_slice())?;
/// assert_eq!(public_key.into_bytes(), wire_bytes);
/// # Ok::<(), rsl_crypto::CryptoError>(())
/// ```
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct X25519PublicKey([u8; X25519_BYTES]);

impl X25519PublicKey {
    /// Size of an encoded X25519 public coordinate in bytes.
    pub const LEN: usize = X25519_BYTES;

    /// Take ownership of one exact-size public coordinate.
    #[must_use]
    pub const fn new(bytes: [u8; X25519_BYTES]) -> Self {
        Self(bytes)
    }

    /// Borrow the received or derived bytes without canonicalizing their encoding.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; X25519_BYTES] {
        &self.0
    }

    /// Return the received or derived bytes without canonicalizing their encoding.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; X25519_BYTES] {
        self.0
    }
}

impl AsRef<[u8]> for X25519PublicKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl TryFrom<&[u8]> for X25519PublicKey {
    type Error = CryptoError;

    /// Copy an exact 32-byte wire slice into a public-coordinate value.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidLength`] for every length other than 32 bytes.
    fn try_from(bytes: &[u8]) -> Result<Self> {
        let actual = bytes.len();
        let bytes =
            <[u8; X25519_BYTES]>::try_from(bytes).map_err(|_| CryptoError::InvalidLength {
                name: "X25519 public key",
                expected: X25519_BYTES,
                actual,
            })?;

        Ok(Self::new(bytes))
    }
}

/// A validated, nonzero X25519 shared secret.
///
/// This owner is non-`Clone`, redacted, and zeroized on drop. It deliberately has no `AsRef`
/// implementation: callers must make exposure visible at the KDF boundary. The raw bytes are not
/// a general-purpose symmetric key and should be consumed by a protocol-specified KDF.
///
/// # Examples
///
/// ```
/// use rsl_crypto::agreement::x25519::{X25519, X25519PrivateKey};
///
/// let alice = X25519PrivateKey::new([0x11; 32]);
/// let bob = X25519PrivateKey::new([0x22; 32]);
/// let bob_public = X25519::public_key(&bob);
/// let shared_secret = X25519::agree(&alice, &bob_public)?;
/// assert_eq!(shared_secret.expose_secret().len(), 32);
/// assert_eq!(format!("{shared_secret:?}"), "X25519SharedSecret([REDACTED])");
/// # Ok::<(), rsl_crypto::CryptoError>(())
/// ```
pub struct X25519SharedSecret {
    bytes: SecretBytes<X25519_BYTES>,
}

impl X25519SharedSecret {
    /// Size of a validated X25519 shared secret in bytes.
    pub const LEN: usize = X25519_BYTES;

    /// Borrow the shared secret explicitly for immediate use by a key-derivation function.
    #[must_use]
    pub fn expose_secret(&self) -> &[u8; X25519_BYTES] {
        self.bytes.expose_secret()
    }

    /// Transfer the bytes to the caller, who becomes responsible for clearing them.
    #[must_use]
    pub fn into_inner(self) -> [u8; X25519_BYTES] {
        self.bytes.into_inner()
    }
}

impl fmt::Debug for X25519SharedSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("X25519SharedSecret([REDACTED])")
    }
}

/// RFC 7748 X25519 public-key derivation and checked Diffie-Hellman agreement.
///
/// This zero-sized type carries no key or protocol state. See the [`x25519` teaching
/// guide](crate::agreement::x25519) for a complete exchange and the exact primitive/protocol
/// boundary.
#[derive(Clone, Copy, Debug, Default)]
pub struct X25519;

impl X25519 {
    /// Derive the RFC 7748 public coordinate `X25519(private_key, 9)`.
    #[must_use]
    pub fn public_key(private_key: &X25519PrivateKey) -> X25519PublicKey {
        X25519PublicKey::new(scalar_multiply(
            private_key.bytes.expose_secret(),
            &BASE_COORDINATE,
        ))
    }

    /// Calculate a shared secret and reject the all-zero result.
    ///
    /// RFC 7748 §6.1 explains that applying bitwise OR to every output byte detects a small-order
    /// input without
    /// leaking more information about the result. A nonzero result is transferred immediately
    /// into a zeroizing owner; a rejected temporary is cleared before returning.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidPublicKey`] when agreement produces the all-zero value.
    pub fn agree(
        private_key: &X25519PrivateKey,
        peer_public_key: &X25519PublicKey,
    ) -> Result<X25519SharedSecret> {
        let mut shared_bytes = scalar_multiply(
            private_key.bytes.expose_secret(),
            peer_public_key.as_bytes(),
        );
        let combined = shared_bytes
            .iter()
            .fold(0_u8, |accumulator, byte| accumulator | byte);

        if combined == 0 {
            shared_bytes.zeroize();
            return Err(CryptoError::InvalidPublicKey);
        }

        Ok(X25519SharedSecret {
            bytes: SecretBytes::new(shared_bytes),
        })
    }
}

impl KeyAgreement for X25519 {
    type PrivateKey = X25519PrivateKey;
    type PublicKey = X25519PublicKey;
    type SharedSecret = X25519SharedSecret;

    fn public_key(private_key: &Self::PrivateKey) -> Self::PublicKey {
        Self::public_key(private_key)
    }

    fn agree(
        private_key: &Self::PrivateKey,
        peer_public_key: &Self::PublicKey,
    ) -> Result<Self::SharedSecret> {
        Self::agree(private_key, peer_public_key)
    }
}

#[cfg(test)]
mod unit {
    use super::*;
    use alloc::format;

    #[test]
    fn public_values_are_exact_size_and_secret_debug_is_redacted() {
        let private_key = X25519PrivateKey::new([0x42; 32]);
        let public_key = X25519::public_key(&private_key);

        assert_eq!(X25519PrivateKey::LEN, 32);
        assert_eq!(X25519PublicKey::LEN, 32);
        assert_eq!(public_key.as_bytes().len(), 32);
        assert_eq!(format!("{private_key:?}"), "X25519PrivateKey([REDACTED])");
    }

    #[test]
    fn public_wire_slices_require_exact_length() {
        assert_eq!(
            X25519PublicKey::try_from([0_u8; 31].as_slice()),
            Err(CryptoError::InvalidLength {
                name: "X25519 public key",
                expected: 32,
                actual: 31,
            })
        );
        assert!(X25519PublicKey::try_from([0_u8; 32].as_slice()).is_ok());
        assert_eq!(
            X25519PublicKey::try_from([0_u8; 33].as_slice()),
            Err(CryptoError::InvalidLength {
                name: "X25519 public key",
                expected: 32,
                actual: 33,
            })
        );
    }

    #[test]
    fn zero_coordinate_is_rejected_as_a_shared_secret() {
        let private_key = X25519PrivateKey::new([0x42; 32]);
        let zero = X25519PublicKey::new([0; 32]);

        assert!(matches!(
            X25519::agree(&private_key, &zero),
            Err(CryptoError::InvalidPublicKey)
        ));
    }
}
