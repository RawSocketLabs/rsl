//! The sending half of the record layer.

// --- Internal modules ---
use super::{Aes128GcmProtection, RecordError};
use crate::wire::{ContentType, ProtocolVersion, RecordHeader};

/// The record layer's write state: the protection in force and the sequence number of the
/// next record (RFC 5246 §6.1).
///
/// This is the guided way to produce records. It owns the sequence number, so one is never
/// sealed twice under a key, which [`Aes128GcmProtection::seal`] alone cannot promise.
pub struct WriteState {
    protection: Option<Aes128GcmProtection>,
    sequence_number: u64,
}

impl WriteState {
    /// Conservative whole-power bound below the RFC 9325 §4.4 recommendation of 2^24.5.
    /// Counts every protected record under this sending key, regardless of content type.
    const KEY_LIMIT: u64 = 1 << 24;

    pub(crate) fn last_key_slot(&self) -> bool {
        self.is_protected() && self.sequence_number == Self::KEY_LIMIT - 1
    }

    pub(crate) fn key_exhausted(&self) -> bool {
        self.is_protected() && self.sequence_number >= Self::KEY_LIMIT
    }

    /// The state a connection starts in: records are sent unprotected.
    #[must_use]
    pub const fn plain() -> Self {
        Self {
            protection: None,
            sequence_number: 0,
        }
    }

    /// Switches to `protection` for every later record and restarts the sequence number, as
    /// sending `ChangeCipherSpec` does (RFC 5246 §6.1).
    pub fn protect(&mut self, protection: Aes128GcmProtection) {
        self.protection = Some(protection);
        self.sequence_number = 0;
    }

    /// Whether records are being protected.
    #[must_use]
    pub const fn is_protected(&self) -> bool {
        self.protection.is_some()
    }

    /// Builds one whole record, header and fragment, carrying `plaintext`.
    ///
    /// # Errors
    ///
    /// [`RecordError::PlaintextTooLong`] when `plaintext` is longer than
    /// [`RecordHeader::MAX_PLAINTEXT_LEN`]; the caller splits longer data across records.
    /// [`RecordError::SequenceExhausted`] when the sequence number would wrap, or
    /// [`RecordError::KeyUsageLimit`] when this key has sealed 2^24 records.
    pub fn seal(
        &mut self,
        content_type: ContentType,
        version: ProtocolVersion,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, RecordError> {
        let mut record = Vec::new();
        self.seal_into(content_type, version, plaintext, &mut record)?;
        Ok(record)
    }

    /// Append one complete record. On error, both the sequence and output stay intact.
    pub(crate) fn seal_into(
        &mut self,
        content_type: ContentType,
        version: ProtocolVersion,
        plaintext: &[u8],
        output: &mut Vec<u8>,
    ) -> Result<(), RecordError> {
        if plaintext.len() > usize::from(RecordHeader::MAX_PLAINTEXT_LEN) {
            return Err(RecordError::PlaintextTooLong);
        }
        if self.key_exhausted() {
            return Err(RecordError::KeyUsageLimit);
        }
        let sequence = self.sequence_number;
        let next = sequence
            .checked_add(1)
            .ok_or(RecordError::SequenceExhausted)?;
        let nonce = sequence.to_be_bytes();
        let sealed = self
            .protection
            .as_ref()
            .map(|key| key.encrypt(nonce, sequence, content_type, version, plaintext))
            .transpose()?;
        let overhead = if sealed.is_some() {
            Aes128GcmProtection::OVERHEAD
        } else {
            0
        };
        let length =
            u16::try_from(plaintext.len() + overhead).map_err(|_| RecordError::PlaintextTooLong)?;
        output.reserve(RecordHeader::LEN + usize::from(length));
        output.extend_from_slice(&[u8::from(content_type), version.major, version.minor]);
        output.extend_from_slice(&length.to_be_bytes());
        if let Some(sealed) = sealed {
            output.extend_from_slice(&nonce);
            output.extend_from_slice(sealed.ciphertext());
            output.extend_from_slice(sealed.tag().as_bytes());
        } else {
            output.extend_from_slice(plaintext);
        }
        self.sequence_number = next;
        Ok(())
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    impl WriteState {
        pub(crate) fn with_two_records_left(protection: Aes128GcmProtection) -> Self {
            Self {
                protection: Some(protection),
                sequence_number: Self::KEY_LIMIT - 2,
            }
        }
    }

    #[test]
    fn protected_records_share_one_budget_and_append_atomically() {
        let mut state =
            WriteState::with_two_records_left(Aes128GcmProtection::new([1; 16], [2; 4]));
        let version = ProtocolVersion::TLS_1_2;
        let mut bytes = vec![0xcc];
        state
            .seal_into(ContentType::Handshake, version, b"one", &mut bytes)
            .unwrap();
        assert!(state.last_key_slot());
        state
            .seal_into(ContentType::Alert, version, b"two", &mut bytes)
            .unwrap();
        assert_eq!(bytes[0], 0xcc);
        let committed = bytes.clone();
        assert_eq!(
            state.seal_into(ContentType::ApplicationData, version, b"three", &mut bytes),
            Err(RecordError::KeyUsageLimit)
        );
        assert_eq!(bytes, committed);
        assert_eq!(state.sequence_number, WriteState::KEY_LIMIT);
        state.protect(Aes128GcmProtection::new([3; 16], [4; 4]));
        assert!(
            state
                .seal(ContentType::ApplicationData, version, b"new key")
                .is_ok()
        );
        assert_eq!(state.sequence_number, 1);
    }

    #[test]
    fn last_sequence_number_is_never_sealed_so_it_cannot_wrap() {
        let mut state = WriteState::plain();
        state.sequence_number = u64::MAX;
        let version = ProtocolVersion::TLS_1_2;
        assert_eq!(
            state.seal(ContentType::ApplicationData, version, b"x"),
            Err(RecordError::SequenceExhausted)
        );
        assert_eq!(state.sequence_number, u64::MAX);
    }
}
