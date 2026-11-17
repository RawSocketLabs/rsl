//! The framed data phase.

// --- Workspace dependencies ---
use sangfor_atrust_wire::tcp::{FrameHeader, FrameKind};

// --- Internal modules ---
use super::Error;

/// The most stream bytes one frame carries.
pub const MAX_FRAME_DATA: usize = 0xffff;

/// `data` as the frames that carry it (spec §9.3): pieces of at most [`MAX_FRAME_DATA`]
/// bytes, each behind its header. No data is no frames.
///
/// The result is to be written whole, with nothing else in between: frames of two writers
/// must not interleave.
#[must_use]
#[expect(
    clippy::missing_panics_doc,
    reason = "the expects state invariants of chunking and of a fixed-size codec"
)]
pub fn frame(data: &[u8]) -> Vec<u8> {
    let mut frames = Vec::with_capacity(data.len() + FrameHeader::LEN);
    for piece in data.chunks(MAX_FRAME_DATA) {
        let length = u16::try_from(piece.len()).expect("invariant: chunks are at most 65535 long");
        let header = FrameHeader::data(length).to_bytes();
        frames.extend(header.expect("invariant: a frame header always encodes"));
        frames.extend_from_slice(piece);
    }
    frames
}

/// The close frame: the sender has finished writing.
#[must_use]
pub(super) fn close_frame() -> Vec<u8> {
    FrameHeader::close()
        .to_bytes()
        .expect("invariant: a frame header always encodes")
}

/// Takes the stream out of the frames a node sends (spec §9.3), from bytes as they arrive
/// and in whatever pieces.
#[derive(Clone, Debug, Default)]
pub struct FrameReader {
    /// The header being collected.
    header: [u8; FrameHeader::LEN],
    /// How many bytes of it are in.
    collected: usize,
    /// How many bytes of the current frame's data are still to come.
    remaining: usize,
    /// How the stream ended, once it has.
    ended: Option<End>,
}

/// How a framed stream ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum End {
    /// A close frame arrived.
    Closed,
    /// Something that is not a frame arrived.
    Broken,
}

impl FrameReader {
    /// A reader at the start of the data phase.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Move stream bytes from the frames in `input` to `output`, as many as both allow.
    /// Returns how much of `input` was used and how much of `output` was filled. It stops
    /// at a close frame, and uses nothing after one.
    ///
    /// An empty data frame carries nothing and is passed over.
    ///
    /// # Errors
    ///
    /// [`Error::Protocol`] when a header is not a frame's. Data read before it in the same
    /// call is returned first. The stream is unusable from then on, and every later call
    /// fails the same way.
    pub fn read(&mut self, input: &[u8], output: &mut [u8]) -> Result<(usize, usize), Error> {
        let (mut used, mut filled) = (0, 0);
        loop {
            match self.ended {
                // What was read before the break is delivered first; the next call reports it.
                Some(End::Broken) if filled > 0 => break,
                Some(End::Broken) => return Err(Error::Protocol("the node sent a bad frame")),
                Some(End::Closed) => break,
                None => {}
            }
            let (input, output) = (&input[used..], &mut output[filled..]);
            if self.remaining > 0 {
                let count = self.remaining.min(input.len()).min(output.len());
                if count == 0 {
                    break;
                }
                output[..count].copy_from_slice(&input[..count]);
                self.remaining -= count;
                used += count;
                filled += count;
                continue;
            }
            let count = (FrameHeader::LEN - self.collected).min(input.len());
            if count == 0 {
                break;
            }
            self.header[self.collected..self.collected + count].copy_from_slice(&input[..count]);
            self.collected += count;
            used += count;
            if self.collected == FrameHeader::LEN {
                self.collected = 0;
                self.begin_frame();
            }
        }
        Ok((used, filled))
    }

    /// Whether a close frame has arrived: the stream has ended, cleanly.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.ended == Some(End::Closed)
    }

    /// Whether the reader stands between two frames. A connection that ends here ended the
    /// stream cleanly; one that ends inside a header or a frame's data cut it short.
    #[must_use]
    pub const fn is_between_frames(&self) -> bool {
        self.collected == 0 && self.remaining == 0
    }

    /// Act on the header just collected.
    fn begin_frame(&mut self) {
        let header = FrameHeader::decode_exact(&self.header).ok();
        let kind = header
            .filter(|header| header.marker == FrameHeader::MARKER)
            .map(|header| (header.kind, header.length));
        match kind {
            Some((FrameKind::Data, length)) => self.remaining = length.into(),
            // The length of a close frame means nothing.
            Some((FrameKind::Close, _)) => self.ended = Some(End::Closed),
            Some((FrameKind::Other(_), _)) | None => self.ended = Some(End::Broken),
        }
    }
}
