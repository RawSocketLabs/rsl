//! Typed AES-256-GCM boundary: the AES-128-GCM profile over the fourteen-round key size.
//!
//! Every GCM layer is shared with [`super::api`]; only the key type and the block cipher differ.
//! TLS 1.3 negotiates this profile as `TLS_AES_256_GCM_SHA384`; SSH as `aes256-gcm@openssh.com`
//! (whose nonce construction stays protocol-owned).

use alloc::vec::Vec;
use core::fmt;
use zeroize::Zeroize;

use super::{open, seal, setup::GcmIv96};
use crate::{
    CryptoError, Result, SecretBytes,
    aead::{Aead, Sealed},
    cipher::aes::aes256::{Aes256, Aes256Key},
    random::RandomSource,
};

/// Number of bytes in an AES-256-GCM key.
const KEY_BYTES: usize = 32;

/// Number of bytes in the supported 96-bit nonce profile.
const NONCE_BYTES: usize = 12;

/// Number of bytes in the supported full authentication tag.
const TAG_BYTES: usize = 16;

/// An owned AES-256 key dedicated to GCM.
///
/// This type intentionally implements neither `Clone`, `Copy`, `AsRef`, nor `Deref`. Constructing
/// [`Aes256Gcm`] consumes it, expands it once, and clears the input owner. Keeping it distinct from
/// [`Aes256Key`] reflects SP 800-38D §5.1's requirement that a GCM key be used exclusively for
/// GCM with its chosen block cipher.
///
/// # Examples
///
/// ```
/// use rsl_crypto::aead::gcm::{Aes256Gcm, Aes256GcmKey};
///
/// let key = Aes256GcmKey::new([0x42; 32]);
/// let algorithm = Aes256Gcm::new(key); // consumes and expands the dedicated key
/// assert_eq!(format!("{algorithm:?}"), "Aes256Gcm([REDACTED KEY SCHEDULE])");
/// ```
pub struct Aes256GcmKey {
    bytes: SecretBytes<KEY_BYTES>,
}

impl Aes256GcmKey {
    /// Size of an AES-256-GCM key in bytes.
    pub const LEN: usize = KEY_BYTES;

    /// Take ownership of exactly 256 key bits.
    #[must_use]
    pub fn new(bytes: [u8; KEY_BYTES]) -> Self {
        Self {
            bytes: SecretBytes::new(bytes),
        }
    }

    /// Generate a dedicated AES-256-GCM key with the caller-selected randomness source.
    ///
    /// The source is supplied explicitly so a `no_std` primitive does not silently choose an
    /// operating-system API, blocking policy, or hardware provider. Integration code must pass a
    /// cryptographically secure source; deterministic implementations are suitable only for
    /// tests.
    ///
    /// # Errors
    ///
    /// Returns the source's error and clears the partially filled temporary key before returning.
    pub fn generate<R: RandomSource>(random: &mut R) -> Result<Self> {
        let mut bytes = [0_u8; KEY_BYTES];
        if let Err(error) = random.fill_bytes(&mut bytes) {
            bytes.zeroize();
            return Err(error);
        }
        Ok(Self::new(bytes))
    }
}

impl fmt::Debug for Aes256GcmKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Aes256GcmKey([REDACTED])")
    }
}

/// One owned 96-bit AES-256-GCM nonce.
///
/// Nonces are not secret and may be transmitted or derived from protocol-visible state. Reusing
/// the same nonce with the same key can destroy GCM's confidentiality and authentication; this
/// value type enforces size, while the consuming protocol must enforce uniqueness.
///
/// # Examples
///
/// ```
/// use rsl_crypto::aead::gcm::Aes256GcmNonce;
///
/// let wire_bytes = [0x24; 12];
/// let nonce = Aes256GcmNonce::try_from(wire_bytes.as_slice())?;
/// assert_eq!(nonce.as_bytes(), &wire_bytes);
/// # Ok::<(), rsl_crypto::CryptoError>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Aes256GcmNonce([u8; NONCE_BYTES]);

impl Aes256GcmNonce {
    /// Size of this nonce profile in bytes.
    pub const LEN: usize = NONCE_BYTES;

    /// Take ownership of exactly 96 nonce bits.
    #[must_use]
    pub const fn new(bytes: [u8; NONCE_BYTES]) -> Self {
        Self(bytes)
    }

    /// Generate one random 96-bit nonce with the caller-selected randomness source.
    ///
    /// Random generation is one permitted way to construct GCM IVs, but this value type cannot
    /// remember every nonce used under a key. The caller remains responsible for the invocation
    /// limits and collision analysis of its random-IV policy. Stateful protocols such as TLS and
    /// SSH should normally derive nonces from their traffic-key and sequence-number rules instead.
    ///
    /// # Errors
    ///
    /// Returns the source's error without constructing a nonce when all twelve bytes cannot be
    /// filled.
    pub fn generate<R: RandomSource>(random: &mut R) -> Result<Self> {
        let mut bytes = [0_u8; NONCE_BYTES];
        random.fill_bytes(&mut bytes)?;
        Ok(Self::new(bytes))
    }

    /// Borrow all twelve nonce bytes in wire order.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; NONCE_BYTES] {
        &self.0
    }

    /// Return the nonce bytes in wire order.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; NONCE_BYTES] {
        self.0
    }
}

impl AsRef<[u8]> for Aes256GcmNonce {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl TryFrom<&[u8]> for Aes256GcmNonce {
    type Error = CryptoError;

    /// Copy an exact 12-byte wire or protocol slice into a typed nonce.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidLength`] for every slice length other than twelve bytes.
    fn try_from(bytes: &[u8]) -> Result<Self> {
        let actual = bytes.len();
        let bytes =
            <[u8; NONCE_BYTES]>::try_from(bytes).map_err(|_| CryptoError::InvalidLength {
                name: "AES-256-GCM nonce",
                expected: NONCE_BYTES,
                actual,
            })?;

        Ok(Self::new(bytes))
    }
}

/// One complete 128-bit AES-256-GCM authentication tag.
///
/// Tags are public protocol values. Exact size is carried by the type, so authenticated
/// decryption cannot accidentally accept a truncated slice.
///
/// # Examples
///
/// ```
/// use rsl_crypto::aead::gcm::Aes256GcmTag;
///
/// let wire_bytes = [0xa5; 16];
/// let tag = Aes256GcmTag::try_from(wire_bytes.as_slice())?;
/// assert_eq!(tag.into_bytes(), wire_bytes);
/// # Ok::<(), rsl_crypto::CryptoError>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Aes256GcmTag([u8; TAG_BYTES]);

impl Aes256GcmTag {
    /// Size of the authentication tag in bytes.
    pub const LEN: usize = TAG_BYTES;

    /// Take ownership of one complete received or stored tag.
    #[must_use]
    pub const fn new(bytes: [u8; TAG_BYTES]) -> Self {
        Self(bytes)
    }

    /// Borrow all sixteen tag bytes in wire order.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; TAG_BYTES] {
        &self.0
    }

    /// Return all sixteen tag bytes in wire order.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; TAG_BYTES] {
        self.0
    }
}

impl AsRef<[u8]> for Aes256GcmTag {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl TryFrom<&[u8]> for Aes256GcmTag {
    type Error = CryptoError;

    /// Copy an exact 16-byte wire or protocol slice into a typed authentication tag.
    ///
    /// # Errors
    ///
    /// Returns [`CryptoError::InvalidLength`] for every slice length other than sixteen bytes.
    fn try_from(bytes: &[u8]) -> Result<Self> {
        let actual = bytes.len();
        let bytes = <[u8; TAG_BYTES]>::try_from(bytes).map_err(|_| CryptoError::InvalidLength {
            name: "AES-256-GCM tag",
            expected: TAG_BYTES,
            actual,
        })?;

        Ok(Self::new(bytes))
    }
}

/// AES-256-GCM with one expanded, zeroizing key schedule.
///
/// The value is deliberately non-`Clone` so copying it cannot silently duplicate expanded key
/// material. It does not track protocol sequence numbers or nonce use; those state transitions
/// remain visible in the TLS, SSH, or other protocol context that owns this primitive.
///
/// # Examples
///
/// ```
/// use rsl_crypto::aead::gcm::{Aes256Gcm, Aes256GcmKey, Aes256GcmNonce};
///
/// let algorithm = Aes256Gcm::new(Aes256GcmKey::new([0x42; 32]));
/// let nonce = Aes256GcmNonce::new([0x24; 12]);
/// let sealed = algorithm.seal(&nonce, b"visible header", b"secret body")?;
/// let opened = algorithm.open(
///     &nonce,
///     b"visible header",
///     sealed.ciphertext(),
///     sealed.tag(),
/// )?;
/// assert_eq!(opened, b"secret body");
/// # Ok::<(), rsl_crypto::CryptoError>(())
/// ```
pub struct Aes256Gcm {
    cipher: Aes256,
}

impl Aes256Gcm {
    /// Consume and expand one dedicated AES-256-GCM key.
    #[must_use]
    pub fn new(key: Aes256GcmKey) -> Self {
        let Aes256GcmKey { bytes } = key;
        let cipher = Aes256::new(Aes256Key::new(bytes.into_inner()));

        Self { cipher }
    }

    /// Encrypt plaintext and authenticate it together with unencrypted associated data.
    ///
    /// The caller must ensure this nonce has never been used for another encryption under this
    /// key. The returned ciphertext has the same length as `plaintext`; its detached tag always
    /// contains sixteen bytes.
    ///
    /// # Errors
    ///
    /// Returns [`crate::CryptoError::MessageTooLong`] before transformation if the AAD or
    /// plaintext exceeds the SP 800-38D input limits.
    ///
    /// # Examples
    ///
    /// ```
    /// use rsl_crypto::aead::gcm::{Aes256Gcm, Aes256GcmKey, Aes256GcmNonce};
    ///
    /// let algorithm = Aes256Gcm::new(Aes256GcmKey::new([0x42; 32]));
    /// let nonce = Aes256GcmNonce::new([0x24; 12]);
    /// let header = b"sent in clear";
    /// let sealed = algorithm.seal(&nonce, header, b"encrypted")?;
    /// assert_eq!(sealed.ciphertext().len(), b"encrypted".len());
    /// assert_eq!(header, b"sent in clear");
    /// # Ok::<(), rsl_crypto::CryptoError>(())
    /// ```
    pub fn seal(
        &self,
        nonce: &Aes256GcmNonce,
        associated_data: &[u8],
        plaintext: &[u8],
    ) -> Result<Sealed<Aes256GcmTag>> {
        let iv = GcmIv96::new(nonce.into_bytes());
        let result = seal::seal(&self.cipher, &iv, associated_data, plaintext)?;
        let (ciphertext, tag) = result.into_parts();

        Ok(Sealed::new(ciphertext, Aes256GcmTag::new(tag)))
    }

    /// Authenticate and decrypt a complete ciphertext.
    ///
    /// Authentication is completed before a plaintext buffer is created. A caller must treat any
    /// error as a rejected record or packet and must not retry it under altered public inputs.
    ///
    /// # Errors
    ///
    /// Returns [`crate::CryptoError::MessageTooLong`] for unsupported AAD/ciphertext lengths or
    /// [`crate::CryptoError::AuthenticationFailed`] when the tag does not authenticate the exact
    /// nonce, AAD, and ciphertext. Authentication failure returns no plaintext.
    ///
    /// # Examples
    ///
    /// ```
    /// use rsl_crypto::{CryptoError, aead::gcm::{Aes256Gcm, Aes256GcmKey, Aes256GcmNonce}};
    ///
    /// let algorithm = Aes256Gcm::new(Aes256GcmKey::new([0x42; 32]));
    /// let nonce = Aes256GcmNonce::new([0x24; 12]);
    /// let sealed = algorithm.seal(&nonce, b"header", b"payload")?;
    /// let mut changed = sealed.ciphertext().to_vec();
    /// changed[0] ^= 1;
    /// assert_eq!(
    ///     algorithm.open(&nonce, b"header", &changed, sealed.tag()),
    ///     Err(CryptoError::AuthenticationFailed),
    /// );
    /// # Ok::<(), rsl_crypto::CryptoError>(())
    /// ```
    pub fn open(
        &self,
        nonce: &Aes256GcmNonce,
        associated_data: &[u8],
        ciphertext: &[u8],
        tag: &Aes256GcmTag,
    ) -> Result<Vec<u8>> {
        let iv = GcmIv96::new(nonce.into_bytes());
        open::open(
            &self.cipher,
            &iv,
            associated_data,
            ciphertext,
            tag.as_bytes(),
        )
    }
}

impl Aead for Aes256Gcm {
    type Nonce = Aes256GcmNonce;
    type Tag = Aes256GcmTag;

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

impl fmt::Debug for Aes256Gcm {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Aes256Gcm([REDACTED KEY SCHEDULE])")
    }
}

#[cfg(test)]
mod unit {
    use alloc::format;

    use super::{Aes256Gcm, Aes256GcmKey, Aes256GcmNonce, Aes256GcmTag};
    use crate::CryptoError;

    #[test]
    fn public_sizes_are_exact_and_byte_exposure_is_explicit() {
        let nonce_bytes = [0x12; Aes256GcmNonce::LEN];
        let tag_bytes = [0x34; Aes256GcmTag::LEN];
        let nonce = Aes256GcmNonce::new(nonce_bytes);
        let tag = Aes256GcmTag::new(tag_bytes);

        assert_eq!(Aes256GcmKey::LEN, 32);
        assert_eq!(nonce.as_bytes(), &nonce_bytes);
        assert_eq!(nonce.into_bytes(), nonce_bytes);
        assert_eq!(tag.as_bytes(), &tag_bytes);
        assert_eq!(tag.into_bytes(), tag_bytes);
    }

    #[test]
    fn key_owning_debug_output_is_redacted() {
        let key = Aes256GcmKey::new([0x5a; 32]);
        assert_eq!(format!("{key:?}"), "Aes256GcmKey([REDACTED])");

        let algorithm = Aes256Gcm::new(key);
        assert_eq!(
            format!("{algorithm:?}"),
            "Aes256Gcm([REDACTED KEY SCHEDULE])"
        );
    }

    #[test]
    fn wire_slices_require_exact_nonce_and_tag_lengths() {
        let nonce_bytes = [0x12; Aes256GcmNonce::LEN];
        let tag_bytes = [0x34; Aes256GcmTag::LEN];

        assert_eq!(
            Aes256GcmNonce::try_from(nonce_bytes.as_slice()),
            Ok(Aes256GcmNonce::new(nonce_bytes))
        );
        assert_eq!(
            Aes256GcmTag::try_from(tag_bytes.as_slice()),
            Ok(Aes256GcmTag::new(tag_bytes))
        );

        assert_eq!(
            Aes256GcmNonce::try_from(&nonce_bytes[..11]),
            Err(CryptoError::InvalidLength {
                name: "AES-256-GCM nonce",
                expected: 12,
                actual: 11,
            })
        );
        assert_eq!(
            Aes256GcmTag::try_from(&tag_bytes[..15]),
            Err(CryptoError::InvalidLength {
                name: "AES-256-GCM tag",
                expected: 16,
                actual: 15,
            })
        );
    }
}
