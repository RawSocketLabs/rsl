//! One whole handshake message.

// --- Internal modules ---
use crate::wire::{HandshakeHeader, HandshakeType};

/// One handshake message as it crosses the wire: its four-byte header and its body.
///
/// The bytes are kept whole because the transcript hash covers them exactly as sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandshakeMessage {
    msg_type: HandshakeType,
    bytes: Vec<u8>,
}

impl HandshakeMessage {
    /// A message of `msg_type` whose body is `body`, with the header that describes it.
    ///
    /// # Panics
    ///
    /// When `body` is longer than a 24-bit length, which no message this crate builds is.
    #[must_use]
    pub fn new(msg_type: HandshakeType, body: &[u8]) -> Self {
        let length = u32::try_from(body.len()).expect("invariant: a body fits a 24-bit length");
        assert!(length < 1 << 24, "invariant: a body fits a 24-bit length");
        let mut bytes = Vec::with_capacity(HandshakeHeader::LEN + body.len());
        bytes.push(u8::from(msg_type));
        bytes.extend(&length.to_be_bytes()[1..]);
        bytes.extend(body);
        Self { msg_type, bytes }
    }

    /// A message from bytes already known to be one header followed by exactly its body.
    pub(super) const fn from_wire(msg_type: HandshakeType, bytes: Vec<u8>) -> Self {
        Self { msg_type, bytes }
    }

    /// Which message this is.
    #[must_use]
    pub const fn msg_type(&self) -> HandshakeType {
        self.msg_type
    }

    /// The whole message, header first: what is sent and what the transcript hashes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The body, which the message's wire type decodes with `decode_exact`.
    #[must_use]
    pub fn body(&self) -> &[u8] {
        &self.bytes[HandshakeHeader::LEN..]
    }
}
