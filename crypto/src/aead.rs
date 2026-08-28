//! Authenticated encryption with associated data (AEAD).
//!
//! AEAD protects both confidentiality and authenticity in one construction. Plaintext becomes
//! ciphertext; additional authenticated data (AAD) remains visible but is bound to the same tag.
//! Record headers, packet sequence fields, or routing metadata are common AAD examples.
//!
//! # Implemented algorithm
//!
//! [`gcm`] provides the readable AES-128-GCM profile and [`chacha20poly1305`] the RFC 8439
//! `AEAD_CHACHA20_POLY1305` profile; both satisfy [`Aead`]. [`record`] adds bounded incremental
//! protection when a complete value should not be held in memory.
//!
//! # Generic use
//!
//! ```
//! use rsl_crypto::{Result, aead::{Aead, gcm::{Aes128Gcm, Aes128GcmKey, Aes128GcmNonce}}};
//!
//! fn protect<A: Aead>(
//!     algorithm: &A,
//!     nonce: &A::Nonce,
//!     header: &[u8],
//!     payload: &[u8],
//! ) -> Result<rsl_crypto::aead::Sealed<A::Tag>> {
//!     algorithm.seal(nonce, header, payload)
//! }
//!
//! let algorithm = Aes128Gcm::new(Aes128GcmKey::new([0x42; 16]));
//! let nonce = Aes128GcmNonce::new([0x24; 12]);
//! let sealed = protect(&algorithm, &nonce, b"visible header", b"secret body")?;
//! assert_ne!(sealed.ciphertext(), b"secret body");
//! # Ok::<(), rsl_crypto::CryptoError>(())
//! ```

pub mod chacha20poly1305;
pub mod gcm;
pub mod record;

pub use record::{
    CounterNonceSequence, DataRecord, FinalRecord, Nonce96, NonceSequence, ReadyRecordBuilder,
    RecordBuilder, RecordBuilderWithSequence, RecordOpenError, RecordOpener, RecordPlaintextSink,
    RecordSealer, RecordSink, RecordWriteError,
};

use alloc::vec::Vec;

use crate::Result;

/// Ciphertext and a detached authentication tag produced by an [`Aead`] implementation.
///
/// Keeping the tag detached leaves wire-layout policy with the protocol. A protocol may encode
/// `AAD || ciphertext || tag`, place the tag before the ciphertext, or store fields separately.
/// See [`Aes128Gcm::seal`](gcm::Aes128Gcm::seal) for a concrete example.
///
/// # Examples
///
/// The container can be inspected by reference or consumed when a wire encoder needs ownership:
///
/// ```
/// use rsl_crypto::aead::gcm::{Aes128Gcm, Aes128GcmKey, Aes128GcmNonce};
///
/// let algorithm = Aes128Gcm::new(Aes128GcmKey::new([0x42; 16]));
/// let nonce = Aes128GcmNonce::new([0x24; 12]);
/// let sealed = algorithm.seal(&nonce, b"clear header", b"private body")?;
///
/// assert_eq!(sealed.ciphertext().len(), b"private body".len());
/// assert_eq!(sealed.tag().as_bytes().len(), 16);
///
/// let (ciphertext, tag) = sealed.into_parts();
/// assert_eq!(ciphertext.len(), b"private body".len());
/// assert_eq!(tag.as_bytes().len(), 16);
/// # Ok::<(), rsl_crypto::CryptoError>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sealed<Tag> {
    ciphertext: Vec<u8>,
    tag: Tag,
}

impl<Tag> Sealed<Tag> {
    /// Construct a detached authenticated-encryption result.
    pub fn new(ciphertext: Vec<u8>, tag: Tag) -> Self {
        Self { ciphertext, tag }
    }

    /// Borrow the ciphertext bytes.
    #[must_use]
    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }

    /// Borrow the detached authentication tag.
    #[must_use]
    pub fn tag(&self) -> &Tag {
        &self.tag
    }

    /// Consume the result into ciphertext and tag.
    #[must_use]
    pub fn into_parts(self) -> (Vec<u8>, Tag) {
        (self.ciphertext, self.tag)
    }
}

/// Authenticated encryption that releases plaintext only after verification succeeds.
///
/// The owned-output contract is deliberate: an implementation may use temporary in-place
/// operations internally, but [`open`](Self::open) cannot hand partially decrypted,
/// unauthenticated bytes to its caller. See the [`aead` module](crate::aead) for generic use and
/// [`gcm`] for the concrete teaching implementation.
pub trait Aead {
    /// The algorithm's nonce type.
    type Nonce: AsRef<[u8]>;

    /// The algorithm's authentication-tag type.
    type Tag: AsRef<[u8]>;

    /// Encrypt and authenticate `plaintext`, binding `associated_data` without encrypting it.
    ///
    /// # Errors
    ///
    /// Returns an algorithm error when a nonce, length, or invocation limit is invalid.
    fn seal(
        &self,
        nonce: &Self::Nonce,
        associated_data: &[u8],
        plaintext: &[u8],
    ) -> Result<Sealed<Self::Tag>>;

    /// Authenticate and decrypt a complete ciphertext.
    ///
    /// Authentication failure returns [`crate::CryptoError::AuthenticationFailed`] and no
    /// plaintext.
    ///
    /// # Errors
    ///
    /// Returns [`crate::CryptoError::AuthenticationFailed`] for an invalid tag and an
    /// algorithm error for an invalid nonce, length, or invocation limit.
    fn open(
        &self,
        nonce: &Self::Nonce,
        associated_data: &[u8],
        ciphertext: &[u8],
        tag: &Self::Tag,
    ) -> Result<Vec<u8>>;
}

#[cfg(test)]
mod unit {
    use super::*;
    use alloc::vec;

    #[test]
    fn detached_result_preserves_both_outputs() {
        let sealed = Sealed::new(vec![1, 2, 3], [4, 5]);
        assert_eq!(sealed.ciphertext(), &[1, 2, 3]);
        assert_eq!(sealed.tag(), &[4, 5]);
        assert_eq!(sealed.into_parts(), (vec![1, 2, 3], [4, 5]));
    }
}
