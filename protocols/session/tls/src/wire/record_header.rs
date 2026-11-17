//! The five-byte header in front of every record.

// --- Workspace dependencies ---
use bnb::bin;

// --- Internal modules ---
use super::{ContentType, ProtocolVersion};

/// The header of one record: content type, version and fragment length (RFC 5246 §6.2.1).
///
/// A header codec, not a record: the fragment that follows is read by the record layer, which
/// knows whether it is protected. `length` is stored verbatim, so decoding never rejects a
/// length above the limits below (dual-use); enforcing them is the record layer's job.
//~ models rfc5246#6.2.1
#[bin(big)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RecordHeader {
    /// The protocol the fragment belongs to.
    pub content_type: ContentType,
    /// The record-layer version.
    pub version: ProtocolVersion,
    /// Length in bytes of the fragment that follows.
    pub length: u16,
}

impl RecordHeader {
    /// The header's encoded length in bytes.
    pub const LEN: usize = 5;
    /// The largest unprotected fragment a compliant sender emits, 2^14 bytes (§6.2.1).
    pub const MAX_PLAINTEXT_LEN: u16 = 1 << 14;
    /// The largest protected fragment a compliant sender emits, 2^14 + 2048 bytes (§6.2.3).
    pub const MAX_CIPHERTEXT_LEN: u16 = Self::MAX_PLAINTEXT_LEN + 2048;

    /// A header with exactly these fields (no validation).
    #[must_use]
    pub const fn new(content_type: ContentType, version: ProtocolVersion, length: u16) -> Self {
        Self {
            content_type,
            version,
            length,
        }
    }
}
