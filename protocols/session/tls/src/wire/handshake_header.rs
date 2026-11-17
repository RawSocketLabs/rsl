//! The four-byte header in front of every handshake message.

// --- Workspace dependencies ---
use bnb::{bin, u24};

// --- Internal modules ---
use super::HandshakeType;

/// The header of one handshake message: its type and body length (RFC 5246 §7.4).
///
/// A header codec: the body is decoded separately, by the message type the header names,
/// once all `length` bytes have arrived (a message may span several records).
//~ models rfc5246#7.4
#[bin(big)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HandshakeHeader {
    /// The message that follows.
    pub msg_type: HandshakeType,
    /// Length in bytes of the body that follows.
    pub length: u24,
}

impl HandshakeHeader {
    /// The header's encoded length in bytes.
    pub const LEN: usize = 4;

    /// A header with exactly these fields (no validation).
    #[must_use]
    pub const fn new(msg_type: HandshakeType, length: u24) -> Self {
        Self { msg_type, length }
    }
}
