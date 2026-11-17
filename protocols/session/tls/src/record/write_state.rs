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
    /// [`RecordError::SequenceExhausted`] when the sequence number would wrap.
    pub fn seal(
        &mut self,
        content_type: ContentType,
        version: ProtocolVersion,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, RecordError> {
        if plaintext.len() > usize::from(RecordHeader::MAX_PLAINTEXT_LEN) {
            return Err(RecordError::PlaintextTooLong);
        }
        let next = self
            .sequence_number
            .checked_add(1)
            .ok_or(RecordError::SequenceExhausted)?;
        let fragment = match &self.protection {
            Some(protection) => {
                protection.seal(self.sequence_number, content_type, version, plaintext)?
            }
            None => plaintext.to_vec(),
        };
        self.sequence_number = next;

        let length = u16::try_from(fragment.len()).map_err(|_| RecordError::PlaintextTooLong)?;
        let mut record = Vec::with_capacity(RecordHeader::LEN + fragment.len());
        record.push(u8::from(content_type));
        record.extend([version.major, version.minor]);
        record.extend(length.to_be_bytes());
        record.extend(fragment);
        Ok(record)
    }
}

#[cfg(test)]
mod unit {
    use super::*;

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
