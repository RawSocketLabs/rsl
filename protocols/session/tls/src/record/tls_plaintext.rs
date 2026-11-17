//! One record's content after protection has been removed.

// --- Internal modules ---
use crate::wire::{ContentType, ProtocolVersion};

/// One record as the layers above the record layer see it: its type, the version in its
/// header, and its unprotected fragment (RFC 5246 §6.2.1, `TLSPlaintext`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TlsPlaintext {
    /// The protocol the fragment belongs to.
    pub content_type: ContentType,
    /// The version the record's header carried.
    pub version: ProtocolVersion,
    /// The unprotected fragment.
    pub fragment: Vec<u8>,
}
