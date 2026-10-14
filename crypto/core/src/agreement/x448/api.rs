//! Typed public boundary for X448 key agreement.
//!
//! ## Standards ownership
//!
//! [RFC 7748 §6.2][rfc-7748] derives a public u-coordinate by applying X448 to the base
//! coordinate five, then applies the same function to a private scalar and the peer coordinate.
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

/// Number of bytes in every X448 scalar, coordinate, and result.
const X448_BYTES: usize = 56;

/// RFC 7748 §4.2's curve448 base u-coordinate `5`, encoded little-endian.
const BASE_COORDINATE: [u8; X448_BYTES] = {
    let mut bytes = [0_u8; X448_BYTES];
    bytes[0] = 5;
    bytes
};

/// One owned 56-byte X448 private scalar input.
///
/// These bytes are stored as generated; RFC 7748 scalar preparation occurs inside every X448
/// operation. The owner is non-`Clone`, redacted, and zeroized on drop. Random generation is
/// deliberately external because the protocol context must choose an approved entropy source and
/// key lifetime.
///
/// # Examples
///
/// ```
/// use rsl_crypto::agreement::x448::{X448, X448PrivateKey};
///
/// let private_key = X448PrivateKey::new([0x42; 56]);
/// let public_key = X448::public_key(&private_key);
/// assert_eq!(public_key.as_bytes().len(), X448PrivateKey::LEN);
/// assert_eq!(format!("{private_key:?}"), "X448PrivateKey([REDACTED])");
/// ```
pub struct X448PrivateKey {
    bytes: SecretBytes<X448_BYTES>,
}

impl X448PrivateKey {
    /// Size of the private scalar input in bytes.
    pub const LEN: usize = X448_BYTES;

    /// Take ownership of 56 random bytes for use as an X448 scalar input.
    #[must_use]
    pub fn new(bytes: [u8; X448_BYTES]) -> Self {
        Self {
            bytes: SecretBytes::new(bytes),
        }
    }
}

impl fmt::Debug for X448PrivateKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("X448PrivateKey([REDACTED])")
    }
}

/// One received or derived 56-byte X448 Montgomery u-coordinate.
///
/// Public keys are not secret. Construction validates only encoded length because RFC 7748 §5
/// requires X448 implementations to accept non-canonical field encodings and coordinates on
/// the twist. [`X448::agree`] rejects the all-zero result that identifies small-order inputs.
///
/// # Examples
///
/// ```
/// use rsl_crypto::agreement::x448::X448PublicKey;
///
/// let wire_bytes = [0x24; 56];
/// let public_key = X448PublicKey::try_from(wire_bytes.as_slice())?;
/// assert_eq!(public_key.into_bytes(), wire_bytes);
/// # Ok::<(), rsl_crypto::CryptoError>(())
/// ```
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct X448PublicKey([u8; X448_BYTES]);

impl X448PublicKey {
    /// Size of an encoded X448 public coordinate in bytes.
    pub const LEN: usize = X448_BYTES;

    /// Take ownership of one exact-size public coordinate.
    #[must_use]
    pub const fn new(bytes: [u8; X448_BYTES]) -> Self {
        Self(bytes)
    }

    /// Borrow the received or derived bytes without canonicalizing their encoding.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; X448_BYTES] {
        &self.0
    }

    /// Return the received or derived bytes without canonicalizing their encoding.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; X448_BYTES] {
        self.0
    }
}

impl AsRef<[u8]> for X448PublicKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl TryFrom<&[u8]> for X448PublicKey {
    type Error = CryptoError;

    /// Copy an exact 56-byte wire slice into a public-coordinate value.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidLength`] for every length other than 32 bytes.
    fn try_from(bytes: &[u8]) -> Result<Self> {
        let actual = bytes.len();
        let bytes =
            <[u8; X448_BYTES]>::try_from(bytes).map_err(|_| CryptoError::InvalidLength {
                name: "X448 public key",
                expected: X448_BYTES,
                actual,
            })?;

        Ok(Self::new(bytes))
    }
}

/// A validated, nonzero X448 shared secret.
///
/// This owner is non-`Clone`, redacted, and zeroized on drop. It deliberately has no `AsRef`
/// implementation: callers must make exposure visible at the KDF boundary. The raw bytes are not
/// a general-purpose symmetric key and should be consumed by a protocol-specified KDF.
///
/// # Examples
///
/// ```
/// use rsl_crypto::agreement::x448::{X448, X448PrivateKey};
///
/// let alice = X448PrivateKey::new([0x11; 56]);
/// let bob = X448PrivateKey::new([0x22; 56]);
/// let bob_public = X448::public_key(&bob);
/// let shared_secret = X448::agree(&alice, &bob_public)?;
/// assert_eq!(shared_secret.expose_secret().len(), 56);
/// assert_eq!(format!("{shared_secret:?}"), "X448SharedSecret([REDACTED])");
/// # Ok::<(), rsl_crypto::CryptoError>(())
/// ```
pub struct X448SharedSecret {
    bytes: SecretBytes<X448_BYTES>,
}

impl X448SharedSecret {
    /// Size of a validated X448 shared secret in bytes.
    pub const LEN: usize = X448_BYTES;

    /// Borrow the shared secret explicitly for immediate use by a key-derivation function.
    #[must_use]
    pub fn expose_secret(&self) -> &[u8; X448_BYTES] {
        self.bytes.expose_secret()
    }

    /// Transfer the bytes to the caller, who becomes responsible for clearing them.
    #[must_use]
    pub fn into_inner(self) -> [u8; X448_BYTES] {
        // 56-byte arrays have no `Default`, so the value is copied out and the owner's drop
        // zeroizes the original before this function returns.
        *self.bytes.expose_secret()
    }
}

impl fmt::Debug for X448SharedSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("X448SharedSecret([REDACTED])")
    }
}

/// RFC 7748 X448 public-key derivation and checked Diffie-Hellman agreement.
///
/// This zero-sized type carries no key or protocol state. See the [`x448` teaching
/// guide](crate::agreement::x448) for a complete exchange and the exact primitive/protocol
/// boundary.
#[derive(Clone, Copy, Debug, Default)]
pub struct X448;

impl X448 {
    /// Derive the RFC 7748 public coordinate `X448(private_key, 5)`.
    #[must_use]
    pub fn public_key(private_key: &X448PrivateKey) -> X448PublicKey {
        X448PublicKey::new(scalar_multiply(
            private_key.bytes.expose_secret(),
            &BASE_COORDINATE,
        ))
    }

    /// Calculate a shared secret and reject the all-zero result.
    ///
    /// RFC 7748 §6.2 explains that applying bitwise OR to every output byte detects a small-order
    /// input without
    /// leaking more information about the result. A nonzero result is transferred immediately
    /// into a zeroizing owner; a rejected temporary is cleared before returning.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidPublicKey`] when agreement produces the all-zero value.
    pub fn agree(
        private_key: &X448PrivateKey,
        peer_public_key: &X448PublicKey,
    ) -> Result<X448SharedSecret> {
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

        Ok(X448SharedSecret {
            bytes: SecretBytes::new(shared_bytes),
        })
    }
}

impl KeyAgreement for X448 {
    type PrivateKey = X448PrivateKey;
    type PublicKey = X448PublicKey;
    type SharedSecret = X448SharedSecret;

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
        let private_key = X448PrivateKey::new([0x42; 56]);
        let public_key = X448::public_key(&private_key);

        assert_eq!(X448PrivateKey::LEN, 56);
        assert_eq!(X448PublicKey::LEN, 56);
        assert_eq!(public_key.as_bytes().len(), 56);
        assert_eq!(format!("{private_key:?}"), "X448PrivateKey([REDACTED])");
    }

    #[test]
    fn public_wire_slices_require_exact_length() {
        assert_eq!(
            X448PublicKey::try_from([0_u8; 55].as_slice()),
            Err(CryptoError::InvalidLength {
                name: "X448 public key",
                expected: 56,
                actual: 55,
            })
        );
        assert!(X448PublicKey::try_from([0_u8; 56].as_slice()).is_ok());
        assert_eq!(
            X448PublicKey::try_from([0_u8; 57].as_slice()),
            Err(CryptoError::InvalidLength {
                name: "X448 public key",
                expected: 56,
                actual: 57,
            })
        );
    }

    #[test]
    fn zero_coordinate_is_rejected_as_a_shared_secret() {
        let private_key = X448PrivateKey::new([0x42; 56]);
        let zero = X448PublicKey::new([0; 56]);

        assert!(matches!(
            X448::agree(&private_key, &zero),
            Err(CryptoError::InvalidPublicKey)
        ));
    }
}
