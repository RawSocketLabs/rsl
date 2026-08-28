//! Typed `AEAD_CHACHA20_POLY1305` key, nonce, tag, and seal/open boundary.
//!
//! ## Standards ownership
//!
//! RFC 8439 §2.8 fixes the inputs: a 256-bit key, a 96-bit nonce, arbitrary AAD and plaintext,
//! and a 128-bit tag. §4 requires nonce uniqueness per key, which remains the protocol's job.

use alloc::vec::Vec;
use core::fmt;
use zeroize::Zeroize;

use super::construction;
use crate::{
    CryptoError, Result, SecretBytes,
    aead::{Aead, Sealed},
    cipher::chacha20::{ChaCha20, ChaCha20Key, ChaCha20Nonce},
    mac::poly1305::Poly1305Tag,
    random::RandomSource,
};

const KEY_BYTES: usize = 32;
const NONCE_BYTES: usize = 12;
const TAG_BYTES: usize = 16;

/// One owned 256-bit key dedicated to `AEAD_CHACHA20_POLY1305`.
///
/// Non-`Clone`, redacted, and zeroized on drop. Distinct from [`ChaCha20Key`] so a key used for
/// the AEAD is never also used for raw keystream under the same nonces.
pub struct ChaCha20Poly1305Key {
    bytes: SecretBytes<KEY_BYTES>,
}

impl ChaCha20Poly1305Key {
    /// Size of the key in bytes.
    pub const LEN: usize = KEY_BYTES;

    /// Take ownership of exactly 256 key bits.
    #[must_use]
    pub fn new(bytes: [u8; KEY_BYTES]) -> Self {
        Self {
            bytes: SecretBytes::new(bytes),
        }
    }

    /// Generate a key with the caller-selected randomness source.
    ///
    /// # Errors
    ///
    /// Returns the source's error and clears the partially filled temporary before returning.
    pub fn generate<R: RandomSource>(random: &mut R) -> Result<Self> {
        let mut bytes = [0_u8; KEY_BYTES];
        if let Err(error) = random.fill_bytes(&mut bytes) {
            bytes.zeroize();
            return Err(error);
        }
        Ok(Self::new(bytes))
    }
}

impl fmt::Debug for ChaCha20Poly1305Key {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ChaCha20Poly1305Key([REDACTED])")
    }
}

/// One 96-bit `AEAD_CHACHA20_POLY1305` nonce.
///
/// Public, but it must never repeat under one key. TLS 1.3 forms it by `XORing` the record
/// sequence number into a per-connection IV; that construction belongs to the protocol.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ChaCha20Poly1305Nonce([u8; NONCE_BYTES]);

impl ChaCha20Poly1305Nonce {
    /// Size of the nonce in bytes.
    pub const LEN: usize = NONCE_BYTES;

    /// Take ownership of exactly 96 nonce bits.
    #[must_use]
    pub const fn new(bytes: [u8; NONCE_BYTES]) -> Self {
        Self(bytes)
    }

    /// Generate a random nonce for a one-shot use where the protocol permits random nonces.
    ///
    /// # Errors
    ///
    /// Returns the source's error.
    pub fn generate<R: RandomSource>(random: &mut R) -> Result<Self> {
        let mut bytes = [0_u8; NONCE_BYTES];
        random.fill_bytes(&mut bytes)?;
        Ok(Self(bytes))
    }

    /// Borrow the nonce bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; NONCE_BYTES] {
        &self.0
    }

    /// Consume the nonce into its bytes.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; NONCE_BYTES] {
        self.0
    }
}

impl AsRef<[u8]> for ChaCha20Poly1305Nonce {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl TryFrom<&[u8]> for ChaCha20Poly1305Nonce {
    type Error = CryptoError;

    fn try_from(bytes: &[u8]) -> Result<Self> {
        let actual = bytes.len();
        let exact =
            <[u8; NONCE_BYTES]>::try_from(bytes).map_err(|_| CryptoError::InvalidLength {
                name: "ChaCha20-Poly1305 nonce",
                expected: NONCE_BYTES,
                actual,
            })?;
        Ok(Self(exact))
    }
}

/// One 128-bit `AEAD_CHACHA20_POLY1305` authentication tag.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ChaCha20Poly1305Tag([u8; TAG_BYTES]);

impl ChaCha20Poly1305Tag {
    /// Size of the tag in bytes.
    pub const LEN: usize = TAG_BYTES;

    /// Take ownership of tag bytes.
    #[must_use]
    pub const fn new(bytes: [u8; TAG_BYTES]) -> Self {
        Self(bytes)
    }

    /// Borrow the tag bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; TAG_BYTES] {
        &self.0
    }

    /// Consume the tag into its bytes.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; TAG_BYTES] {
        self.0
    }
}

impl AsRef<[u8]> for ChaCha20Poly1305Tag {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl TryFrom<&[u8]> for ChaCha20Poly1305Tag {
    type Error = CryptoError;

    fn try_from(bytes: &[u8]) -> Result<Self> {
        let actual = bytes.len();
        let exact = <[u8; TAG_BYTES]>::try_from(bytes).map_err(|_| CryptoError::InvalidLength {
            name: "ChaCha20-Poly1305 tag",
            expected: TAG_BYTES,
            actual,
        })?;
        Ok(Self(exact))
    }
}

/// RFC 8439 `AEAD_CHACHA20_POLY1305` keyed for sealing and opening.
///
/// See the [`chacha20poly1305` teaching page](crate::aead::chacha20poly1305) for the published
/// example and the primitive/protocol boundary.
pub struct ChaCha20Poly1305 {
    cipher: ChaCha20,
}

impl ChaCha20Poly1305 {
    /// Consume one dedicated key.
    #[must_use]
    pub fn new(key: ChaCha20Poly1305Key) -> Self {
        Self {
            cipher: ChaCha20::new(ChaCha20Key::new(key.bytes.into_inner())),
        }
    }

    /// Encrypt plaintext and authenticate it together with unencrypted associated data.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::MessageTooLong`] before transformation if the plaintext exceeds
    /// `2^38 - 64` bytes.
    ///
    /// # Examples
    ///
    /// ```
    /// use rsl_crypto::aead::chacha20poly1305::{
    ///     ChaCha20Poly1305, ChaCha20Poly1305Key, ChaCha20Poly1305Nonce,
    /// };
    ///
    /// let algorithm = ChaCha20Poly1305::new(ChaCha20Poly1305Key::new([0x42; 32]));
    /// let nonce = ChaCha20Poly1305Nonce::new([0x24; 12]);
    /// let sealed = algorithm.seal(&nonce, b"header", b"payload")?;
    /// assert_eq!(sealed.ciphertext().len(), 7);
    /// assert_eq!(algorithm.open(&nonce, b"header", sealed.ciphertext(), sealed.tag())?, b"payload");
    /// # Ok::<(), rsl_crypto::CryptoError>(())
    /// ```
    pub fn seal(
        &self,
        nonce: &ChaCha20Poly1305Nonce,
        associated_data: &[u8],
        plaintext: &[u8],
    ) -> Result<Sealed<ChaCha20Poly1305Tag>> {
        let (ciphertext, tag) = construction::seal(
            &self.cipher,
            &ChaCha20Nonce::new(nonce.into_bytes()),
            associated_data,
            plaintext,
        )?;
        Ok(Sealed::new(
            ciphertext,
            ChaCha20Poly1305Tag(tag.into_bytes()),
        ))
    }

    /// Authenticate and decrypt a complete ciphertext; plaintext exists only after success.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::MessageTooLong`] for unsupported lengths or
    /// [`CryptoError::AuthenticationFailed`] when the tag does not authenticate the exact nonce,
    /// AAD, and ciphertext.
    pub fn open(
        &self,
        nonce: &ChaCha20Poly1305Nonce,
        associated_data: &[u8],
        ciphertext: &[u8],
        tag: &ChaCha20Poly1305Tag,
    ) -> Result<Vec<u8>> {
        construction::open(
            &self.cipher,
            &ChaCha20Nonce::new(nonce.into_bytes()),
            associated_data,
            ciphertext,
            &Poly1305Tag::new(tag.into_bytes()),
        )
    }
}

impl Aead for ChaCha20Poly1305 {
    type Nonce = ChaCha20Poly1305Nonce;
    type Tag = ChaCha20Poly1305Tag;

    fn seal(
        &self,
        nonce: &Self::Nonce,
        associated_data: &[u8],
        plaintext: &[u8],
    ) -> Result<Sealed<Self::Tag>> {
        Self::seal(self, nonce, associated_data, plaintext)
    }

    fn open(
        &self,
        nonce: &Self::Nonce,
        associated_data: &[u8],
        ciphertext: &[u8],
        tag: &Self::Tag,
    ) -> Result<Vec<u8>> {
        Self::open(self, nonce, associated_data, ciphertext, tag)
    }
}

impl fmt::Debug for ChaCha20Poly1305 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ChaCha20Poly1305([REDACTED KEY])")
    }
}

#[cfg(test)]
mod unit {
    use super::*;
    use alloc::format;

    #[test]
    fn secret_owners_are_redacted_and_wire_types_check_lengths() {
        let key = ChaCha20Poly1305Key::new([0x42; 32]);
        assert_eq!(format!("{key:?}"), "ChaCha20Poly1305Key([REDACTED])");
        let algorithm = ChaCha20Poly1305::new(key);
        assert_eq!(format!("{algorithm:?}"), "ChaCha20Poly1305([REDACTED KEY])");
        assert_eq!(
            ChaCha20Poly1305Nonce::try_from([0_u8; 8].as_slice()),
            Err(CryptoError::InvalidLength {
                name: "ChaCha20-Poly1305 nonce",
                expected: 12,
                actual: 8,
            })
        );
        assert_eq!(
            ChaCha20Poly1305Tag::try_from([0_u8; 15].as_slice()),
            Err(CryptoError::InvalidLength {
                name: "ChaCha20-Poly1305 tag",
                expected: 16,
                actual: 15,
            })
        );
    }
}
