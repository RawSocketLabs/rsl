use bnb::{BitBuf, BitError, ErrorKind, Source};
use std::io;

// RFC 1929 §2: VER + ULEN + 255 username bytes + PLEN + 255 password bytes.
// This also covers every RFC 1928 greeting, selection, request, and reply.
pub(crate) const MAX_FRAME_LEN: usize = 513;

/// A SOCKS transport together with any prefetched application bytes.
///
/// Reads drain the buffered prefix before reading the underlying transport. Writes
/// and flushes go directly to it. Implements standard I/O with `blocking` or `mio`, and Tokio
/// I/O with `tokio` (requiring an `Unpin` transport).
///
/// Keep this wrapper when handing the connection to another protocol. To change
/// adapters, transfer **both** values from [`into_parts`](Self::into_parts), or use
/// [`try_into_inner`](Self::try_into_inner) once no buffered input remains.
/// There is deliberately no `Debug`: buffer storage may retain consumed credentials.
/// Neither dropping nor extracting the buffer zeroizes its storage.
///
/// Handshakes retain at most 513 unread bytes. This limit does not restrict raw
/// application reads or buffers explicitly supplied through [`from_parts`](Self::from_parts).
/// Empty reads do not access the transport. A read that returns a buffered prefix
/// does not also read the transport, even if the caller supplied additional space.
pub struct Stream<S> {
    pub(crate) inner: S,
    pub(crate) buffered: BitBuf,
}

impl<S> Stream<S> {
    pub(crate) fn new(inner: S) -> Self {
        Self::from_parts(inner, BitBuf::bounded(MAX_FRAME_LEN))
    }

    /// Borrow the underlying transport for configuration or inspection.
    /// Reading through it bypasses the buffered prefix and can reorder input.
    pub fn get_ref(&self) -> &S {
        &self.inner
    }

    /// Mutably borrow the transport, for example to adjust timeouts.
    /// Direct reads bypass buffered input; use the wrapper for application I/O.
    pub fn get_mut(&mut self) -> &mut S {
        &mut self.inner
    }

    /// Extract a transport only when no unread bits remain.
    ///
    /// # Errors
    /// Returns the unchanged stream if buffered input still needs to be delivered.
    pub fn try_into_inner(self) -> Result<S, Self> {
        if self.buffered.is_empty() {
            Ok(self.inner)
        } else {
            Err(self)
        }
    }

    /// Separate the transport and buffered input without copying or discarding it.
    /// The buffer retains its cursor and capacity. Both parts must be transferred.
    pub fn into_parts(self) -> (S, BitBuf) {
        (self.inner, self.buffered)
    }

    /// Reconstruct a stream from an existing transport and its unread input.
    ///
    /// The buffer is not reset or normalized. Raw reads reject a partial-byte
    /// cursor with `InvalidData` without consuming it; resolve bit alignment through
    /// bnb before byte-oriented handoff. This does not negotiate a SOCKS session.
    pub fn from_parts(inner: S, buffered: BitBuf) -> Self {
        Self { inner, buffered }
    }

    pub(super) fn buffered_len(&self) -> io::Result<usize> {
        let position = self.buffered.bit_pos();
        if position % 8 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                BitError::new(ErrorKind::NotByteAligned, position),
            ));
        }
        Ok(self.buffered.bit_len() / 8)
    }

    pub(super) fn read_buffered(&mut self, bytes: &mut [u8]) -> io::Result<()> {
        self.buffered
            .read_into(bytes)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
    }
}
