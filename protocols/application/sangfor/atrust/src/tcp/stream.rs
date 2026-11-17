//! A tunnel as a blocking byte stream.

// --- Standard library ---
use std::fmt;
use std::io::{self, Read, Write};

// --- Internal modules ---
use super::frames::close_frame;
use super::{
    Destination, Error, FrameReader, Handshake, Identity, MAX_FRAME_DATA, Mode, Progress, frame,
};

/// How much is read from the node at a time.
const CHUNK: usize = 16 * 1024;

/// A connection to a node: a byte stream that can stop writing while it goes on reading.
///
/// In practice this is a TLS connection, and stopping is its `close_notify` (spec §2.1).
/// A plain `Read + Write` cannot say that, and a tunnel needs it to end its half of a
/// stream. Deadlines are the transport's too: the reference client gives the node 18 seconds
/// to answer the handshake (spec §9).
pub trait Transport: Read + Write {
    /// Tell the peer this end will write nothing more, and keep the read side open.
    ///
    /// # Errors
    ///
    /// Any failure to send that notice.
    fn close_write(&mut self) -> io::Result<()>;
}

/// One TCP stream to a host behind the gateway (spec §9), over one connection to a node.
///
/// Reading yields what the far host sends and `Ok(0)` when it has finished; writing sends
/// to it. [`close_write`](Self::close_write) ends this end's half and leaves reading open.
pub struct Stream<T> {
    /// The connection to the node.
    transport: T,
    /// How the data phase works.
    mode: Mode,
    /// The state of the framed data phase. Unused in raw mode.
    frames: FrameReader,
    /// Bytes received from the node and not yet delivered.
    received: Vec<u8>,
    /// How much of `received` has been delivered.
    delivered: usize,
    /// Where this end's half of the stream stands.
    write_side: WriteSide,
}

/// Where the writing half of a stream stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WriteSide {
    /// Data can be written.
    Open,
    /// This end has finished, and said so.
    Closed,
    /// A frame went out in part. What the node reads from here on is not what was meant, so
    /// nothing more is sent.
    Broken,
}

impl<T: Transport> Stream<T> {
    /// Open a stream to `destination` over `transport`, a fresh connection to the node that
    /// serves it, on behalf of `identity`. `zero_rtt` is the gateway's capability from its
    /// manifest.
    ///
    /// # Errors
    ///
    /// [`Error::Io`] when the connection fails or ends before the node has answered, and
    /// what [`Handshake`] reports otherwise.
    pub fn open(
        mut transport: T,
        identity: &Identity,
        destination: &Destination,
        zero_rtt: bool,
    ) -> Result<Self, Error> {
        let (mut handshake, first) = Handshake::new(identity, destination, zero_rtt)?;
        transport.write_all(&first)?;
        transport.flush()?;

        let mut chunk = vec![0; CHUNK];
        loop {
            let count = read_some(&mut transport, &mut chunk)?;
            if count == 0 {
                return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into());
            }
            if let Progress::Open { mode, early } = handshake.receive(&chunk[..count])? {
                return Ok(Self {
                    transport,
                    mode,
                    frames: FrameReader::new(),
                    received: early,
                    delivered: 0,
                    write_side: WriteSide::Open,
                });
            }
        }
    }

    /// How the data phase works.
    #[must_use]
    pub const fn mode(&self) -> Mode {
        self.mode
    }

    /// The connection to the node.
    #[must_use]
    pub const fn get_ref(&self) -> &T {
        &self.transport
    }

    /// Finish writing: the far host sees the end of this end's half, and reading goes on
    /// (spec §9.3). When the node agreed to it, that is a close frame and the connection
    /// stays open; otherwise it is the transport's own notice. Calling this again after it
    /// succeeded does nothing.
    ///
    /// # Errors
    ///
    /// Any failure to send the notice, after which the call may be made again, unless a
    /// frame went out in part: then, and after a data frame did, `BrokenPipe`.
    pub fn close_write(&mut self) -> io::Result<()> {
        match self.write_side {
            WriteSide::Closed => return Ok(()),
            WriteSide::Broken => return Err(io::ErrorKind::BrokenPipe.into()),
            WriteSide::Open => {}
        }
        if self.mode.reusable {
            self.send_frames(&close_frame())?;
        } else {
            self.transport.close_write()?;
        }
        self.write_side = WriteSide::Closed;
        Ok(())
    }

    /// Send whole frames and flush them. A failure may have left part of a frame on the
    /// wire, so it ends the writing half for good.
    fn send_frames(&mut self, frames: &[u8]) -> io::Result<()> {
        let sent = self
            .transport
            .write_all(frames)
            .and_then(|()| self.transport.flush());
        if sent.is_err() {
            self.write_side = WriteSide::Broken;
        }
        sent
    }

    /// The bytes received and not yet delivered.
    fn waiting(&self) -> &[u8] {
        &self.received[self.delivered..]
    }

    /// Note that `count` more of the received bytes are delivered.
    fn deliver(&mut self, count: usize) {
        self.delivered += count;
        if self.delivered == self.received.len() {
            self.received.clear();
            self.delivered = 0;
        }
    }

    /// Read more from the node into the emptied buffer. Returns how much came.
    fn refill(&mut self) -> io::Result<usize> {
        self.received.resize(CHUNK, 0);
        let count = read_some(&mut self.transport, &mut self.received);
        self.received.truncate(*count.as_ref().unwrap_or(&0));
        count
    }
}

impl<T> fmt::Debug for Stream<T> {
    /// Shows how the stream works and whether this end has finished, and none of the data
    /// passing through it.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Stream")
            .field("mode", &self.mode)
            .field("write_side", &self.write_side)
            .finish_non_exhaustive()
    }
}

impl<T: Transport> Read for Stream<T> {
    /// Reads stream data. `Ok(0)` is the end of the far host's half: the transport's end in
    /// raw mode, a close frame or the transport ending between frames in framed mode.
    ///
    /// # Errors
    ///
    /// The transport's errors; `InvalidData` when the node sends something that is not a
    /// frame; and `UnexpectedEof` when the connection ends inside a frame.
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        if !self.mode.framed {
            // What arrived with the handshake comes first.
            if self.waiting().is_empty() {
                return self.transport.read(output);
            }
            let count = self.waiting().len().min(output.len());
            output[..count].copy_from_slice(&self.waiting()[..count]);
            self.deliver(count);
            return Ok(count);
        }
        loop {
            // First what is already here, which also reports a break remembered from before.
            let (used, filled) = self
                .frames
                .read(&self.received[self.delivered..], output)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            self.deliver(used);
            if filled > 0 || self.frames.is_closed() {
                return Ok(filled);
            }
            if self.waiting().is_empty() && self.refill()? == 0 {
                return if self.frames.is_between_frames() {
                    Ok(0)
                } else {
                    Err(io::ErrorKind::UnexpectedEof.into())
                };
            }
        }
    }
}

impl<T: Transport> Write for Stream<T> {
    /// Writes stream data. In framed mode each call sends and flushes one frame, of at most
    /// one frame's worth of data. A write of nothing sends nothing.
    ///
    /// # Errors
    ///
    /// The transport's errors, and `BrokenPipe` after
    /// [`close_write`](Stream::close_write) or after a frame went out in part: a framed
    /// write that fails cannot be retried, because the node may hold half a frame.
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if self.write_side != WriteSide::Open {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        if data.is_empty() {
            return Ok(0);
        }
        if !self.mode.framed {
            return self.transport.write(data);
        }
        let piece = &data[..data.len().min(MAX_FRAME_DATA)];
        self.send_frames(&frame(piece))?;
        Ok(piece.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.transport.flush()
    }
}

/// One read that is not interrupted.
fn read_some(transport: &mut impl Read, buffer: &mut [u8]) -> io::Result<usize> {
    loop {
        match transport.read(buffer) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            result => return result,
        }
    }
}
