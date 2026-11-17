//! Joining handshake messages out of record fragments.

// --- Workspace dependencies ---
use thiserror::Error;

// --- Internal modules ---
use super::HandshakeMessage;
use crate::wire::{HandshakeHeader, HandshakeType};

/// A handshake message announced a body longer than [`HandshakeJoiner::MAX_BODY_LEN`].
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
#[error("{msg_type:?} handshake message of {length} bytes is longer than allowed")]
pub struct MessageTooLong {
    /// The message's type.
    pub msg_type: HandshakeType,
    /// The body length its header announced.
    pub length: usize,
}

/// Joins handshake messages back together from the fragments of handshake records.
///
/// A message may span records and a record may hold several messages (RFC 5246 §6.2.1), so
/// fragments are buffered and whole messages taken out. The buffer is bounded: a message
/// longer than [`Self::MAX_BODY_LEN`] is refused when its header arrives, before its body is
/// buffered.
#[derive(Debug, Default)]
pub struct HandshakeJoiner {
    buffer: Vec<u8>,
}

impl HandshakeJoiner {
    /// The longest body accepted, 64 KiB. The wire allows 16 MiB; nothing a client receives
    /// in a handshake needs more than a certificate chain, which is a few kilobytes.
    pub const MAX_BODY_LEN: usize = 64 * 1024;

    /// A joiner holding nothing.
    #[must_use]
    pub const fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    /// Adds the fragment of one handshake record.
    pub fn push(&mut self, fragment: &[u8]) {
        self.buffer.extend_from_slice(fragment);
    }

    /// Whether no bytes of a message are waiting. This must hold whenever the keys change:
    /// a message may not straddle a `ChangeCipherSpec`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Takes the next whole message, or returns `None` when more fragments are needed.
    ///
    /// # Errors
    ///
    /// [`MessageTooLong`] when the next message's header announces too long a body.
    pub fn next_message(&mut self) -> Result<Option<HandshakeMessage>, MessageTooLong> {
        let Some((header, _)) = self.buffer.split_first_chunk::<{ HandshakeHeader::LEN }>() else {
            return Ok(None);
        };
        let msg_type = HandshakeType::from(header[0]);
        let length =
            usize::from(header[1]) << 16 | usize::from(header[2]) << 8 | usize::from(header[3]);
        if length > Self::MAX_BODY_LEN {
            return Err(MessageTooLong { msg_type, length });
        }
        let end = HandshakeHeader::LEN + length;
        if self.buffer.len() < end {
            return Ok(None);
        }
        let bytes = self.buffer.drain(..end).collect();
        Ok(Some(HandshakeMessage::from_wire(msg_type, bytes)))
    }
}
