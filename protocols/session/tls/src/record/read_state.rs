//! The receiving half of the record layer.

// --- Internal modules ---
use super::{Aes128GcmProtection, RecordError, TlsPlaintext};
use crate::wire::RecordHeader;

/// The record layer's read state: bytes received but not yet framed, the protection in
/// force, and the sequence number of the next record (RFC 5246 §6.1).
///
/// The buffer holds at most one record: [`receive`](Self::receive) takes bytes only up to the
/// end of the record it is reading, and the caller takes that record out with
/// [`next_record`](Self::next_record) before offering what follows. Bytes of later records
/// therefore stay with the caller, where a paused connection cannot lose track of them.
pub struct ReadState {
    buffer: Vec<u8>,
    protection: Option<Aes128GcmProtection>,
    sequence_number: u64,
}

impl ReadState {
    /// The most bytes the buffer holds: one protected record of the largest size.
    pub const CAPACITY: usize = RecordHeader::LEN + RecordHeader::MAX_CIPHERTEXT_LEN as usize;

    /// The state a connection starts in: records arrive unprotected.
    #[must_use]
    pub const fn plain() -> Self {
        Self {
            buffer: Vec::new(),
            protection: None,
            sequence_number: 0,
        }
    }

    /// Switches to `protection` for every later record and restarts the sequence number, as
    /// receiving `ChangeCipherSpec` does (RFC 5246 §6.1).
    pub fn protect(&mut self, protection: Aes128GcmProtection) {
        self.protection = Some(protection);
        self.sequence_number = 0;
    }

    /// Whether records are expected to be protected.
    #[must_use]
    pub const fn is_protected(&self) -> bool {
        self.protection.is_some()
    }

    /// How many more bytes complete the header or the record being read. Zero means a whole
    /// record (or one too long to accept) is waiting for [`next_record`](Self::next_record).
    #[must_use]
    pub fn receive_limit(&self) -> usize {
        let Some((header, _)) = self.buffer.split_first_chunk::<{ RecordHeader::LEN }>() else {
            return RecordHeader::LEN - self.buffer.len();
        };
        let length = u16::from_be_bytes([header[3], header[4]]);
        if length > RecordHeader::MAX_CIPHERTEXT_LEN {
            return 0;
        }
        (RecordHeader::LEN + usize::from(length)).saturating_sub(self.buffer.len())
    }

    /// Buffers bytes from the peer, stopping at the end of the record being read, and
    /// returns how many were taken. Offer the rest again after
    /// [`next_record`](Self::next_record).
    pub fn receive(&mut self, bytes: &[u8]) -> usize {
        let mut taken = 0;
        // Two rounds at most: the rest of the header, then the body it announces.
        while taken < bytes.len() {
            let step = self.receive_limit().min(bytes.len() - taken);
            if step == 0 {
                break;
            }
            self.buffer.extend_from_slice(&bytes[taken..taken + step]);
            taken += step;
        }
        taken
    }

    /// Whether bytes of an unfinished record are buffered. A connection that ends in this
    /// state was cut short.
    #[must_use]
    pub fn has_partial_record(&self) -> bool {
        !self.buffer.is_empty()
    }

    /// Takes the next complete record out of the buffer and removes its protection, or
    /// returns `None` when more bytes are needed.
    ///
    /// The length in a record's header is checked as soon as the header has arrived, so an
    /// oversized record is refused before its body is buffered.
    ///
    /// # Errors
    ///
    /// [`RecordError::RecordOverflow`] when a record or its plaintext is longer than allowed,
    /// [`RecordError::SequenceExhausted`] when the sequence number would wrap, and the
    /// errors of [`Aes128GcmProtection::open`]. Every error is final for the connection.
    pub fn next_record(&mut self) -> Result<Option<TlsPlaintext>, RecordError> {
        let Some((header, _)) = self.buffer.split_first_chunk::<{ RecordHeader::LEN }>() else {
            return Ok(None);
        };
        let header = RecordHeader::decode_exact(header).map_err(|_| RecordError::Truncated)?;
        let limit = match self.protection {
            Some(_) => RecordHeader::MAX_CIPHERTEXT_LEN,
            None => RecordHeader::MAX_PLAINTEXT_LEN,
        };
        if header.length > limit {
            return Err(RecordError::RecordOverflow);
        }
        let end = RecordHeader::LEN + usize::from(header.length);
        let Some(fragment) = self.buffer.get(RecordHeader::LEN..end) else {
            return Ok(None);
        };
        let next = self
            .sequence_number
            .checked_add(1)
            .ok_or(RecordError::SequenceExhausted)?;
        let fragment = match &self.protection {
            Some(protection) => protection.open(
                self.sequence_number,
                header.content_type,
                header.version,
                fragment,
            )?,
            None => fragment.to_vec(),
        };
        if fragment.len() > usize::from(RecordHeader::MAX_PLAINTEXT_LEN) {
            return Err(RecordError::RecordOverflow);
        }
        self.sequence_number = next;
        self.buffer.drain(..end);
        Ok(Some(TlsPlaintext {
            content_type: header.content_type,
            version: header.version,
            fragment,
        }))
    }
}
