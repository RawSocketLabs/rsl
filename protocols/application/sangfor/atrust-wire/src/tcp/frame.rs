//! The frames of a TCP tunnel's framed data phase.

// --- Workspace dependencies ---
use bnb::{BitEnum, bin};

/// What a frame of the framed data phase is. Dual-use: any other value is preserved.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[bit_enum(u8)]
#[repr(u8)]
pub enum FrameKind {
    /// Stream data follows.
    Data = 0x00,
    /// The sender has finished writing. The length field carries no meaning.
    Close = 0x01,
    /// Any other kind, preserved verbatim.
    #[catch_all]
    Other(u8),
}

/// The four-byte header of one frame in the framed data phase of a TCP tunnel (spec §9.3):
/// a marker, the frame's kind, and the length of the data that follows.
///
/// A header codec: the data is read separately, `length` bytes of it for a data frame.
#[bin(big)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FrameHeader {
    /// The marker, 1.
    pub marker: u8,
    /// What the frame is.
    pub kind: FrameKind,
    /// How many bytes of data follow a data frame.
    pub length: u16,
}

impl FrameHeader {
    /// The header's encoded length in bytes.
    pub const LEN: usize = 4;
    /// The marker byte every frame starts with.
    pub const MARKER: u8 = 0x01;

    /// The header of a data frame carrying `length` bytes.
    #[must_use]
    pub const fn data(length: u16) -> Self {
        Self {
            marker: Self::MARKER,
            kind: FrameKind::Data,
            length,
        }
    }

    /// The close frame, which is a header alone.
    #[must_use]
    pub const fn close() -> Self {
        Self {
            marker: Self::MARKER,
            kind: FrameKind::Close,
            length: 0,
        }
    }
}
