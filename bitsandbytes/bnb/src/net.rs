//! Ergonomic `std` socket helpers — the `net` feature.
//!
//! Two wrappers so you exchange `#[bin]` messages instead of bytes + buffers, one per transport
//! shape:
//!   * [`MessageStream`] — whole-message read/write over any `Read + Write` (a *byte stream*,
//!     e.g. a `TcpStream`). It owns the stream and buffers reads, so one value does both
//!     directions (no `try_clone`); messages must be **self-delimiting** (their `#[bin]`
//!     structure, a `magic`, or a length prefix bounds them).
//!   * [`MessageDatagram`] — whole-message send/recv over any [`DatagramSocket`] (a
//!     *message-oriented* socket where one recv is one whole message, e.g. a `UdpSocket` or a
//!     `UnixDatagram`). It owns the socket and reuses one receive buffer.
//!
//! [`DatagramSocket`] is the datagram counterpart to `Read + Write` that std doesn't ship — so
//! `MessageDatagram` is generic across UDP, Unix datagram sockets, and anything you implement it
//! for (a raw socket, a mock). Both wrappers bridge `std::io::Error` into [`BitError`] (the
//! `std` feature), so a single `?` covers I/O *and* codec errors.

use crate::{BitBuf, BitDecode, BitEncode, BitError, BitReader, BitWriter};
use alloc::vec;
use alloc::vec::Vec;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, UdpSocket};

/// Encode any message to a fresh `Vec` (generic over [`BitEncode`], unlike the inherent
/// `to_bytes`).
fn encode<T: BitEncode>(msg: &T) -> Result<Vec<u8>, BitError> {
    let mut w = BitWriter::with_layout(<T as BitEncode>::LAYOUT);
    msg.bit_encode(&mut w)?;
    Ok(w.into_bytes())
}

/// A whole-message reader/writer over a byte stream (anything `Read + Write`, e.g. a
/// `TcpStream`). It owns the stream and keeps a read buffer, so [`read_message`] and
/// [`write_message`] exchange `#[bin]` values — and one `MessageStream` serves *both*
/// directions on a single connection, no `try_clone` needed.
///
/// [`read_message`]: MessageStream::read_message
/// [`write_message`]: MessageStream::write_message
#[derive(Debug)]
pub struct MessageStream<S> {
    inner: S,
    buf: BitBuf,
}

impl<S> MessageStream<S> {
    /// Wrap a stream.
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            buf: BitBuf::new(),
        }
    }

    /// Borrow the underlying stream (e.g. to set a timeout).
    pub fn get_mut(&mut self) -> &mut S {
        &mut self.inner
    }

    /// Recover the underlying stream (any buffered-but-unparsed bytes are dropped).
    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<S: Read> MessageStream<S> {
    /// Read exactly one `#[bin]` message, pulling more bytes from the stream as needed and
    /// keeping any trailing bytes for the next call. The message's own byte/bit order is honored
    /// (via [`BitBuf::pull`]).
    ///
    /// # Errors
    /// A codec [`BitError`] for a malformed message, or an I/O error — an EOF mid-stream (a
    /// closed connection) surfaces as an `Io(UnexpectedEof)` error, so a read loop ends on `Err`.
    pub fn read_message<T: BitDecode + BitEncode>(&mut self) -> Result<T, BitError> {
        loop {
            // `pull` decodes in `T`'s own layout, returns `None` until a whole message is
            // buffered, and reclaims consumed bytes — the framing logic lives in `BitBuf`.
            if let Some(msg) = self.buf.pull::<T>()? {
                return Ok(msg);
            }
            let mut chunk = [0u8; 4096];
            let n = self.inner.read(&mut chunk)?;
            if n == 0 {
                return Err(
                    io::Error::new(io::ErrorKind::UnexpectedEof, "connection closed").into(),
                );
            }
            self.buf.push(&chunk[..n]);
        }
    }
}

impl<S: Write> MessageStream<S> {
    /// Encode one `#[bin]` message and write it to the stream.
    ///
    /// # Errors
    /// A codec [`BitError`] or an I/O write error.
    pub fn write_message<T: BitEncode>(&mut self, msg: &T) -> Result<(), BitError> {
        self.inner.write_all(&encode(msg)?)?;
        Ok(())
    }
}

/// A message-oriented (datagram) socket: each `recv_from` yields exactly one whole message with
/// its sender, and each `send_to` writes one message to a peer. This is the datagram counterpart
/// to `Read + Write` (which std *does* ship but has no datagram analog of) — implementing it for
/// a transport makes that transport usable with [`MessageDatagram`].
///
/// Implemented here for [`UdpSocket`] and (on Unix) `std::os::unix::net::UnixDatagram`.
pub trait DatagramSocket {
    /// The peer-address type (`SocketAddr` for UDP; `std::os::unix::net::SocketAddr` for Unix).
    type Addr;

    /// Receive one datagram into `buf`, returning how many bytes it held and who sent it.
    ///
    /// # Errors
    /// An I/O receive error.
    fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, Self::Addr)>;

    /// Send `buf` as one datagram to `addr`, returning the bytes sent.
    ///
    /// # Errors
    /// An I/O send error.
    fn send_to(&self, buf: &[u8], addr: &Self::Addr) -> io::Result<usize>;
}

impl DatagramSocket for UdpSocket {
    type Addr = SocketAddr;

    fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        UdpSocket::recv_from(self, buf)
    }

    fn send_to(&self, buf: &[u8], addr: &SocketAddr) -> io::Result<usize> {
        UdpSocket::send_to(self, buf, *addr)
    }
}

#[cfg(unix)]
impl DatagramSocket for std::os::unix::net::UnixDatagram {
    type Addr = std::os::unix::net::SocketAddr;

    fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, Self::Addr)> {
        std::os::unix::net::UnixDatagram::recv_from(self, buf)
    }

    fn send_to(&self, buf: &[u8], addr: &Self::Addr) -> io::Result<usize> {
        self.send_to_addr(buf, addr)
    }
}

/// A whole-message sender/receiver over a [`DatagramSocket`] (a `UdpSocket`, a `UnixDatagram`,
/// or your own). It owns the socket and reuses one receive buffer, so each datagram is exchanged
/// as a `#[bin]` value — the datagram counterpart to [`MessageStream`]. Unlike a stream, a
/// datagram socket talks to *many* peers, so every call carries the peer address.
#[derive(Debug)]
pub struct MessageDatagram<D> {
    sock: D,
    buf: Vec<u8>,
}

impl<D> MessageDatagram<D> {
    /// Wrap a datagram socket, with a receive buffer sized for the largest datagram (64 KiB).
    pub fn new(sock: D) -> Self {
        Self::with_capacity(sock, 65_536)
    }

    /// Wrap a socket with a receive buffer of `capacity` bytes. A datagram larger than this is
    /// truncated (as the OS itself would truncate an oversized `recv`).
    pub fn with_capacity(sock: D, capacity: usize) -> Self {
        Self {
            sock,
            buf: vec![0u8; capacity],
        }
    }

    /// Borrow the underlying socket (e.g. for `connect`, multicast, or a read timeout).
    pub fn get_ref(&self) -> &D {
        &self.sock
    }

    /// Mutably borrow the underlying socket.
    pub fn get_mut(&mut self) -> &mut D {
        &mut self.sock
    }

    /// Recover the underlying socket.
    pub fn into_inner(self) -> D {
        self.sock
    }
}

impl<D: DatagramSocket> MessageDatagram<D> {
    /// Encode `msg` and send it as one datagram to `addr`. Returns the bytes sent.
    ///
    /// # Errors
    /// A codec [`BitError`] or an I/O send error.
    pub fn send_message<T: BitEncode>(&self, msg: &T, addr: &D::Addr) -> Result<usize, BitError> {
        Ok(self.sock.send_to(&encode(msg)?, addr)?)
    }

    /// Receive one datagram and decode it as a `T`, with the sender's address.
    ///
    /// # Errors
    /// A codec [`BitError`] (the datagram wasn't a valid `T`) or an I/O receive error.
    pub fn recv_message<T: BitDecode + BitEncode>(&mut self) -> Result<(T, D::Addr), BitError> {
        let (n, from) = self.sock.recv_from(&mut self.buf)?;
        // Decode in `T`'s own byte/bit order (not the reader's default).
        let mut r = BitReader::with_layout(&self.buf[..n], <T as BitEncode>::LAYOUT);
        let msg = <T as BitDecode>::bit_decode(&mut r)?;
        Ok((msg, from))
    }
}
