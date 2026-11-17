//! The running hash of a handshake.

// --- Workspace dependencies ---
use crypto::digest::sha2::sha256::Sha256;

// --- Internal modules ---
use super::HandshakeMessage;

/// The SHA-256 hash of every handshake message so far, in the order sent or received
/// (RFC 5246 §7.4.9, `handshake_messages`).
///
/// The caller decides what belongs in it: `HelloRequest` does not (RFC 5246 §7.4.1.1), and
/// neither do `ChangeCipherSpec` or alerts, which are not handshake messages.
#[derive(Clone)]
pub struct Transcript {
    hash: Sha256,
}

impl Transcript {
    /// A transcript of no messages.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            hash: Sha256::new(),
        }
    }

    /// Adds one whole message, header and body.
    ///
    /// # Panics
    ///
    /// Only if the messages added exceed SHA-256's 2^61-byte limit, which the joiner's
    /// per-message bound puts out of reach of any real handshake.
    pub fn add(&mut self, message: &HandshakeMessage) {
        self.hash
            .update(message.bytes())
            .expect("invariant: a handshake is far shorter than SHA-256's 2^61-byte limit");
    }

    /// The hash of everything added so far. More messages may be added afterwards.
    #[must_use]
    pub fn hash(&self) -> [u8; 32] {
        self.hash.clone().finalize().into_bytes()
    }
}

impl Default for Transcript {
    fn default() -> Self {
        Self::new()
    }
}
