//! What USB control machines exchange with their executors: command frames out, correlated
//! whole-transfer events back. The machines never own transport buffers.

/// `OUT_REGOUT_SIZE` in the pinned C transport: one command transfer, header included.
pub(crate) const MAX_FRAME_BYTES: usize = 256;

/// One command transfer for endpoint 0x01: a register header and its payload words, held
/// inline so preparing a command never allocates. Only the first `len` bytes are sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Frame {
    /// Command bytes, little-endian words.
    bytes: [u8; MAX_FRAME_BYTES],
    /// Bytes in use: a whole number of words, at least the header.
    len: usize,
}

impl Frame {
    /// A frame of `header` followed by `words`, which the caller has bounded to fit.
    pub(crate) fn new(header: [u8; 4], words: &[u32]) -> Self {
        let mut bytes = [0; MAX_FRAME_BYTES];
        bytes[..4].copy_from_slice(&header);
        for (chunk, word) in bytes[4..].chunks_exact_mut(4).zip(words) {
            chunk.copy_from_slice(&word.to_le_bytes());
        }
        Self {
            bytes,
            len: (words.len() + 1) * 4,
        }
    }

    /// The bytes to send.
    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

/// The I²C machine's fixed 12-byte command, as a frame for executors that send either kind.
impl From<[u8; 12]> for Frame {
    fn from(packet: [u8; 12]) -> Self {
        let mut bytes = [0; MAX_FRAME_BYTES];
        bytes[..12].copy_from_slice(&packet);
        Self { bytes, len: 12 }
    }
}

/// A correlated executor result; the machine never owns transport buffers.
pub(crate) enum Event<'a, E> {
    /// The command transfer completed successfully.
    Written { id: u64 },
    /// One complete register readback transfer (endpoint 0x81), borrowed for this transition.
    Readback { id: u64, packet: &'a [u8] },
    /// One complete notification transfer (endpoint 0x82), borrowed only for this transition.
    Notification { id: u64, packet: &'a [u8] },
    /// The identified transfer failed; hardware state may be uncertain.
    Failed { id: u64, error: E },
}

impl<E> Event<'_, E> {
    /// The operation identity supplied with the transfer action.
    pub(crate) fn id(&self) -> u64 {
        match self {
            Self::Written { id }
            | Self::Readback { id, .. }
            | Self::Notification { id, .. }
            | Self::Failed { id, .. } => *id,
        }
    }
}
