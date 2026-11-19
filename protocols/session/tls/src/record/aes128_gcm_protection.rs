//! AES-128-GCM record protection.

// --- Workspace dependencies ---
use crypto::aead::Sealed;
use crypto::aead::gcm::{Aes128Gcm, Aes128GcmKey, Aes128GcmNonce, Aes128GcmTag};

// --- Internal modules ---
use super::RecordError;
use crate::wire::{ContentType, ProtocolVersion};

/// One direction's AES-128-GCM record protection (RFC 5288 §3, RFC 5246 §6.2.3.3).
///
/// A protected fragment is an 8-byte explicit nonce, the ciphertext, then a 16-byte tag. The
/// GCM nonce is the 4-byte salt from the key block followed by the explicit nonce. The tag
/// also covers the record's sequence number, type, version and plaintext length, none of
/// which this type tracks: the record layer passes them in, so this type holds no state and
/// one sequence number must never be sealed twice.
//~ models rfc5288#3
//~ models rfc5246#6.2.3.3
pub struct Aes128GcmProtection {
    cipher: Aes128Gcm,
    salt: [u8; Self::SALT_LEN],
}

impl Aes128GcmProtection {
    /// The key's length in bytes.
    pub const KEY_LEN: usize = 16;
    /// The salt's length in bytes: the part of the nonce that is not sent.
    pub const SALT_LEN: usize = 4;
    /// The explicit nonce's length in bytes: the part of the nonce at the front of a fragment.
    pub const EXPLICIT_NONCE_LEN: usize = 8;
    /// The tag's length in bytes.
    pub const TAG_LEN: usize = 16;
    /// How many bytes longer a protected fragment is than its plaintext.
    pub const OVERHEAD: usize = Self::EXPLICIT_NONCE_LEN + Self::TAG_LEN;
    /// The longest plaintext whose protected fragment still fits a record's 16-bit length.
    /// This is what can be framed, not what a compliant sender emits (2^14 bytes).
    pub const MAX_PLAINTEXT_LEN: usize = u16::MAX as usize - Self::OVERHEAD;

    /// Protection with this key and salt.
    #[must_use]
    pub fn new(key: [u8; Self::KEY_LEN], salt: [u8; Self::SALT_LEN]) -> Self {
        Self {
            cipher: Aes128Gcm::new(Aes128GcmKey::new(key)),
            salt,
        }
    }

    /// Protection from a key and salt cut out of a key block.
    pub(crate) fn from_slices(key: &[u8], salt: &[u8]) -> Self {
        let key = key
            .try_into()
            .expect("invariant: the key block is cut into 16-byte keys");
        let salt = salt
            .try_into()
            .expect("invariant: the key block is cut into 4-byte salts");
        Self::new(key, salt)
    }

    /// Protects one record, using its sequence number as the explicit nonce, which RFC 5288 §3
    /// suggests because it never repeats under one key.
    ///
    /// # Errors
    ///
    /// [`RecordError::PlaintextTooLong`] when the protected fragment would not fit a record's
    /// 16-bit length.
    pub fn seal(
        &self,
        sequence_number: u64,
        content_type: ContentType,
        version: ProtocolVersion,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, RecordError> {
        let explicit_nonce = sequence_number.to_be_bytes();
        self.seal_with_explicit_nonce(
            explicit_nonce,
            sequence_number,
            content_type,
            version,
            plaintext,
        )
    }

    /// Protects one record with a caller-chosen explicit nonce.
    ///
    /// This is the raw path: reusing an explicit nonce under one key destroys GCM's
    /// confidentiality and integrity for the records that share it. Use [`Self::seal`] unless
    /// reproducing another implementation's bytes or testing a peer.
    ///
    /// # Errors
    ///
    /// [`RecordError::PlaintextTooLong`] when the protected fragment would not fit a record's
    /// 16-bit length.
    pub fn seal_with_explicit_nonce(
        &self,
        explicit_nonce: [u8; Self::EXPLICIT_NONCE_LEN],
        sequence_number: u64,
        content_type: ContentType,
        version: ProtocolVersion,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, RecordError> {
        let sealed = self.encrypt(
            explicit_nonce,
            sequence_number,
            content_type,
            version,
            plaintext,
        )?;
        Ok([
            explicit_nonce.as_slice(),
            sealed.ciphertext(),
            sealed.tag().as_bytes(),
        ]
        .concat())
    }

    /// Encrypt before committing the record header, explicit nonce and tag to the send queue.
    pub(super) fn encrypt(
        &self,
        explicit_nonce: [u8; Self::EXPLICIT_NONCE_LEN],
        sequence_number: u64,
        content_type: ContentType,
        version: ProtocolVersion,
        plaintext: &[u8],
    ) -> Result<Sealed<Aes128GcmTag>, RecordError> {
        if plaintext.len() > Self::MAX_PLAINTEXT_LEN {
            return Err(RecordError::PlaintextTooLong);
        }
        let aad = additional_data(sequence_number, content_type, version, plaintext.len())?;
        Ok(self
            .cipher
            .seal(&self.nonce(explicit_nonce), &aad, plaintext)?)
    }

    /// Opens one protected fragment and returns its plaintext.
    ///
    /// # Errors
    ///
    /// [`RecordError::Truncated`] when the fragment is shorter than [`Self::OVERHEAD`],
    /// [`RecordError::BadRecordMac`] when it does not authenticate under this key with this
    /// sequence number, type and version, [`RecordError::PlaintextTooLong`] when it is longer
    /// than any record can be, and [`RecordError::Crypto`] if the cipher fails otherwise. A
    /// receiver answers every one of them with a fatal `bad_record_mac` alert (RFC 5288 §3).
    pub fn open(
        &self,
        sequence_number: u64,
        content_type: ContentType,
        version: ProtocolVersion,
        fragment: &[u8],
    ) -> Result<Vec<u8>, RecordError> {
        let (explicit_nonce, protected) = fragment
            .split_first_chunk::<{ Self::EXPLICIT_NONCE_LEN }>()
            .ok_or(RecordError::Truncated)?;
        let (ciphertext, tag) = protected
            .split_last_chunk::<{ Self::TAG_LEN }>()
            .ok_or(RecordError::Truncated)?;
        let aad = additional_data(sequence_number, content_type, version, ciphertext.len())?;
        let nonce = self.nonce(*explicit_nonce);
        let tag = Aes128GcmTag::new(*tag);
        let plaintext = self.cipher.open(&nonce, &aad, ciphertext, &tag)?;
        Ok(plaintext)
    }

    /// The GCM nonce for a record: the salt, then the record's explicit nonce.
    fn nonce(&self, explicit_nonce: [u8; Self::EXPLICIT_NONCE_LEN]) -> Aes128GcmNonce {
        let mut nonce = [0; Self::SALT_LEN + Self::EXPLICIT_NONCE_LEN];
        nonce[..Self::SALT_LEN].copy_from_slice(&self.salt);
        nonce[Self::SALT_LEN..].copy_from_slice(&explicit_nonce);
        Aes128GcmNonce::new(nonce)
    }
}

/// What the tag covers besides the ciphertext: the sequence number, then the record's type,
/// version and plaintext length as they would appear in an unprotected header
/// (RFC 5246 §6.2.3.3).
fn additional_data(
    sequence_number: u64,
    content_type: ContentType,
    version: ProtocolVersion,
    plaintext_len: usize,
) -> Result<[u8; 13], RecordError> {
    let length = u16::try_from(plaintext_len).map_err(|_| RecordError::PlaintextTooLong)?;
    let mut aad = [0; 13];
    aad[..8].copy_from_slice(&sequence_number.to_be_bytes());
    aad[8] = u8::from(content_type);
    aad[9] = version.major;
    aad[10] = version.minor;
    aad[11..].copy_from_slice(&length.to_be_bytes());
    Ok(aad)
}
