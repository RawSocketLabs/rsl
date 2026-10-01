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
//! `MessageDatagram` is generic across UDP and Unix datagram sockets (and, under the `mock`
//! feature, `MockDatagramSocket`; the trait is *sealed*, so those are the only impls).
//! Stream reads use [`MessageReadError`](crate::net::MessageReadError) to preserve both codec context and the original
//! transport error. Writes and datagrams retain their existing [`BitError`] API.
//! Already own your transport and [`BitBuf`]? Use [`read_message`](crate::net::read_message) or, with `tokio-io`,
//! `read_message_async` with caller-owned reusable scratch space.

use crate::{BitBuf, BitDecode, BitEncode, BitError, BitWriter, ErrorKind};
use alloc::vec;
use alloc::vec::Vec;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, UdpSocket};
#[cfg(feature = "mock")]
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
};

/// Encode any message to a fresh `Vec` (generic over [`BitEncode`], unlike the inherent
/// `to_bytes`).
fn encode<T: BitEncode>(msg: &T) -> Result<Vec<u8>, BitError> {
    let mut w = BitWriter::with_layout(<T as BitEncode>::LAYOUT);
    msg.bit_encode(&mut w)?;
    Ok(w.into_bytes())
}

/// A message-reader failure, preserving codec context or the original transport error.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MessageReadError {
    /// Decoding, finite truncation, or retained-buffer capacity failure.
    #[error("message decode failed: {0}")]
    Codec(#[from] BitError),
    /// Transport failure, clean EOF before a message, or invalid scratch space.
    #[error("message read failed: {0}")]
    Io(#[from] io::Error),
}

fn shortfall(error: BitError) -> Result<usize, MessageReadError> {
    match error.kind {
        ErrorKind::Incomplete { needed } => Ok(needed.unwrap_or(1).max(1)),
        _ => Err(error.into()),
    }
}

fn read_limit(buffer: &BitBuf, scratch: &[u8]) -> Result<usize, MessageReadError> {
    if buffer.read_capacity() == Some(0) {
        Err(BitError::new(
            ErrorKind::BufferFull {
                cap: buffer.capacity().expect("bounded buffer"),
            },
            crate::Source::bit_pos(buffer),
        )
        .into())
    } else if scratch.is_empty() {
        Err(io::Error::new(io::ErrorKind::InvalidInput, "message read scratch is empty").into())
    } else {
        Ok(buffer
            .read_capacity()
            .unwrap_or(scratch.len())
            .min(scratch.len()))
    }
}

fn message_at_eof<T: BitDecode + BitEncode>(buffer: &mut BitBuf) -> Result<T, MessageReadError> {
    buffer.pull_eof()?.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "connection closed before a message",
        )
        .into()
    })
}

/// Read one independently byte-padded message using borrowed transport, buffer, and scratch.
///
/// Tries buffered decoding first, then reads until the blocked operation's additional-byte
/// lower bound is met before retrying. Unknown/zero hints wait for at least one byte.
/// If appending compacts/rebases the buffer cursor, the previous hint is invalidated and
/// decoding retries immediately (absolute positions can now mean something different).
/// Read-ahead is allowed: every accepted byte is immediately appended to `buffer`, including
/// later messages or raw protocol data. No scratch allocation is made; decoded values may
/// allocate. A bounded buffer limits retained wire bytes, not codec work or output allocations.
///
/// Messages must be self-delimiting. The message's own layout is used; its final unused bits
/// are accepted and consumed, matching independent encoding. Use [`BitBuf::try_pull`] directly
/// for tightly bit-packed messages, custom context, or your own read scheduling. Decode
/// callbacks may repeat; they must be retry-safe and honor [`ErrorKind::Incomplete`]'s contract.
/// This is not parser continuation or rollback of callback side effects.
///
/// # Errors
/// Codec errors preserve the retained input and codec context. A full buffer returns
/// `Codec(BufferFull)` **before** another read, even when a hint exceeds available capacity.
/// Empty scratch is `Io(InvalidInput)` only if refill is needed (a full buffer takes priority).
/// Interrupted reads retry; other transport errors retain their original owned source.
/// Physical EOF immediately makes one finite attempt, regardless of an outstanding hint;
/// empty EOF is `Io(UnexpectedEof)`, truncation is a codec error. Accepted bytes survive errors.
/// The caller owns timeouts and recovery policy; this function adds no deadline.
///
/// # Panics
/// Panics if `Read` reports more bytes than its destination can hold.
pub fn read_message<T: BitDecode + BitEncode, R: Read + ?Sized>(
    reader: &mut R,
    buffer: &mut BitBuf,
    scratch: &mut [u8],
) -> Result<T, MessageReadError> {
    let message = 'message: loop {
        let mut missing = match buffer.try_pull::<T>() {
            Ok(message) => break message,
            Err(error) => shortfall(error)?,
        };
        while missing != 0 {
            let limit = read_limit(buffer, scratch)?;
            let count = match reader.read(&mut scratch[..limit]) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                result => result?,
            };
            if count == 0 {
                break 'message message_at_eof(buffer)?;
            }
            let start = crate::Source::bit_pos(buffer);
            buffer
                .push(&scratch[..count])
                .expect("read cannot exceed reserved input capacity");
            missing = if crate::Source::bit_pos(buffer) == start {
                missing.saturating_sub(count)
            } else {
                0 // Rebased absolute positions invalidate the previous attempt's hint.
            };
        }
    };
    buffer.finish_byte();
    Ok(message)
}

/// Async counterpart of [`read_message`], using Tokio `AsyncRead` and caller-owned storage.
///
/// Uses the same hints, read-ahead, capacity, EOF, layout, and error contracts. It does not
/// require `tokio-util` or own a runtime. There is no await between a successful read and
/// appending its bytes: dropping this future while a read is pending retains all previously
/// accepted input in `buffer`. Resuming with the same reader and buffer starts a fresh decode
/// attempt, not a suspended parser. This guarantee relies on the underlying `AsyncRead`
/// contract; it does not make a larger protocol session or partial writes cancellation-safe.
///
/// # Errors
/// See [`read_message`]. Configure deadlines/cancellation at the caller or transport.
///
/// # Panics
/// Panics if the underlying `AsyncRead` violates its buffer contract.
#[cfg(feature = "tokio-io")]
pub async fn read_message_async<
    T: BitDecode + BitEncode,
    R: tokio::io::AsyncRead + Unpin + ?Sized,
>(
    reader: &mut R,
    buffer: &mut BitBuf,
    scratch: &mut [u8],
) -> Result<T, MessageReadError> {
    use tokio::io::AsyncReadExt;

    let message = 'message: loop {
        let mut missing = match buffer.try_pull::<T>() {
            Ok(message) => break message,
            Err(error) => shortfall(error)?,
        };
        while missing != 0 {
            let limit = read_limit(buffer, scratch)?;
            let count = match reader.read(&mut scratch[..limit]).await {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                result => result?,
            };
            if count == 0 {
                break 'message message_at_eof(buffer)?;
            }
            let start = crate::Source::bit_pos(buffer);
            buffer
                .push(&scratch[..count])
                .expect("read cannot exceed reserved input capacity");
            missing = if crate::Source::bit_pos(buffer) == start {
                missing.saturating_sub(count)
            } else {
                0 // Same rebase rule as the synchronous reader.
            };
        }
    };
    buffer.finish_byte();
    Ok(message)
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
    /// Wrap a stream with an **unbounded** read buffer. On an untrusted peer prefer
    /// [`bounded`](Self::bounded) — a stream that never completes a frame would otherwise
    /// grow the buffer without bound.
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            buf: BitBuf::new(),
        }
    }

    /// Wrap a stream with a **bounded** read buffer: a peer that streams bytes which never
    /// complete a message can only grow the buffer to `cap` bytes before
    /// [`read_message`](Self::read_message) fails with
    /// [`ErrorKind::BufferFull`], rather than consuming memory
    /// without bound. The bounded counterpart to [`new`](Self::new) for untrusted streams.
    pub fn bounded(inner: S, cap: usize) -> Self {
        Self {
            inner,
            buf: BitBuf::bounded(cap),
        }
    }

    /// Borrow the underlying stream for configuration or inspection. Reading it directly
    /// bypasses retained bytes, even if `S` permits reads through a shared reference.
    pub fn get_ref(&self) -> &S {
        &self.inner
    }

    /// Mutably borrow the underlying stream (e.g. to set a timeout). Direct reads bypass
    /// retained bytes; use this wrapper's `Read` implementation for a protocol handoff.
    pub fn get_mut(&mut self) -> &mut S {
        &mut self.inner
    }

    /// Recover the stream only when no unread bits remain. On failure returns the entire
    /// wrapper, without losing bytes. Use [`into_parts`](Self::into_parts) to transfer both.
    ///
    /// # Errors
    /// Returns `self` if buffered input remains.
    pub fn try_into_inner(self) -> Result<S, Self> {
        if self.buf.is_empty() {
            Ok(self.inner)
        } else {
            Err(self)
        }
    }

    /// Transfer the stream and all retained input, including its bit cursor and capacity.
    pub fn into_parts(self) -> (S, BitBuf) {
        (self.inner, self.buf)
    }

    /// Reconstruct a wrapper without copying or discarding retained input.
    pub fn from_parts(inner: S, buf: BitBuf) -> Self {
        Self { inner, buf }
    }
}

impl<S: Read> MessageStream<S> {
    /// Read exactly one `#[bin]` message, pulling more bytes from the stream as needed and
    /// keeping any trailing bytes for the next call. The message's own byte/bit order is honored
    /// (via [`BitBuf::try_pull`]). Messages are independently byte-padded, like `write_message`:
    /// the unused bits of the last byte are accepted and consumed. Use `BitBuf` directly
    /// for bit-packed concatenation. Honors additional-byte hints internally; see
    /// [`read_message`] for scheduling, scratch-based borrowing, and callback requirements.
    ///
    /// # Errors
    /// A codec error for malformed/truncated input, or an I/O error. Clean connection close
    /// is `Io(UnexpectedEof)`; EOF with retained input makes one finite decode attempt.
    /// At a full configured cap, `BufferFull` is returned **before** any further read.
    /// Errors retain accepted bytes; they do not undo codec side effects.
    ///
    /// # Panics
    /// Panics if the underlying `Read` violates its contract by reporting more bytes
    /// than its destination can hold.
    pub fn read_message<T: BitDecode + BitEncode>(&mut self) -> Result<T, MessageReadError> {
        read_message(&mut self.inner, &mut self.buf, &mut [0; 4096])
    }
}

impl<S: Read> Read for MessageStream<S> {
    /// Return retained whole bytes first, without also reading the underlying stream.
    /// An unaligned imported cursor returns `InvalidData` without consuming anything.
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        let n = self
            .buf
            .read_buffered(out)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if n != 0 { Ok(n) } else { self.inner.read(out) }
    }
}

impl<S: Write> Write for MessageStream<S> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.inner.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

impl<S: Write> MessageStream<S> {
    /// Encode one `#[bin]` message and write it to the stream.
    ///
    /// # Errors
    /// A codec [`BitError`] or an I/O write error. A failed write may have sent a prefix;
    /// retrying the whole message can duplicate it. This method does not flush.
    pub fn write_message<T: BitEncode>(&mut self, msg: &T) -> Result<(), BitError> {
        self.inner.write_all(&encode(msg)?)?;
        Ok(())
    }
}

/// A message-oriented (datagram) socket: each `recv_from` yields exactly one whole message with
/// its sender, and each `send_to` writes one message to a peer. This is the datagram counterpart
/// to `Read + Write` (which std *does* ship but has no datagram analog of) — it makes a transport
/// usable with [`MessageDatagram`].
///
/// **Sealed:** `bnb` implements it for [`UdpSocket`], (on Unix) `std::os::unix::net::UnixDatagram`,
/// and — under the `mock` feature — `MockDatagramSocket`. Downstream crates can't add their own
/// impls, so `bnb` keeps the freedom to evolve the trait; to test datagram code, use
/// `MockDatagramSocket` (the `mock` feature) or a loopback `UdpSocket`.
pub trait DatagramSocket: sealed::Sealed {
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

/// Seals [`DatagramSocket`] — only `bnb`'s own types can implement it (the module is private).
mod sealed {
    pub trait Sealed {}
}

impl sealed::Sealed for UdpSocket {}
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
impl sealed::Sealed for std::os::unix::net::UnixDatagram {}
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

/// A whole-message sender/receiver over a [`DatagramSocket`] (a `UdpSocket`, a `UnixDatagram`, or
/// — under the `mock` feature — a `MockDatagramSocket`). It owns the socket and reuses one
/// receive buffer, so each datagram is exchanged
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
        let msg = crate::bitstream::decode_exact(&self.buf[..n], T::LAYOUT)?;
        Ok((msg, from))
    }
}

/// A test-only [`DatagramSocket`] backed by in-memory queues — exchange datagrams with a
/// [`MessageDatagram`] in unit tests, no real socket bound. Enabled by the **`mock`** feature
/// (put it in your `[dev-dependencies]`). Queue inbound datagrams with
/// [`push_inbound`](Self::push_inbound) (each is one `recv_from`) and inspect what was sent with
/// [`sent`](Self::sent).
///
/// ```
/// use bnb::{bin, MessageDatagram, MockDatagramSocket};
/// #[bin(big)]
/// #[derive(Debug, PartialEq, Eq)]
/// struct Ping {
///     seq: u16,
/// }
///
/// let mut peer = MessageDatagram::new(MockDatagramSocket::new());
/// let from = "127.0.0.1:5000".parse().unwrap();
/// peer.get_ref().push_inbound(&Ping { seq: 7 }.to_bytes().unwrap(), from); // as if it arrived
///
/// let (ping, who): (Ping, _) = peer.recv_message().unwrap();
/// assert_eq!(ping, Ping { seq: 7 });
/// peer.send_message(&Ping { seq: 8 }, &who).unwrap(); // reply to the sender
/// assert_eq!(peer.get_ref().sent()[0].0, Ping { seq: 8 }.to_bytes().unwrap());
/// ```
#[cfg(feature = "mock")]
#[derive(Debug, Default)]
pub struct MockDatagramSocket {
    inbound: RefCell<VecDeque<(Vec<u8>, SocketAddr)>>,
    sent: RefCell<Vec<(Vec<u8>, SocketAddr)>>,
    fail_recv: Cell<bool>,
}

#[cfg(feature = "mock")]
impl MockDatagramSocket {
    /// An empty mock with no queued datagrams.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue one datagram (`bytes`, from `from`) to be returned by the next `recv_from`.
    pub fn push_inbound(&self, bytes: &[u8], from: SocketAddr) {
        self.inbound.borrow_mut().push_back((bytes.to_vec(), from));
    }

    /// Every datagram sent so far, as `(bytes, destination)`, in send order.
    #[must_use]
    pub fn sent(&self) -> Vec<(Vec<u8>, SocketAddr)> {
        self.sent.borrow().clone()
    }

    /// Make the next `recv_from` fail with `ConnectionReset` instead of returning a datagram — to
    /// test recv-error handling. One-shot: later recvs behave normally.
    #[must_use]
    pub fn fail_next_recv(self) -> Self {
        self.fail_recv.set(true);
        self
    }
}

#[cfg(feature = "mock")]
impl sealed::Sealed for MockDatagramSocket {}

#[cfg(feature = "mock")]
impl DatagramSocket for MockDatagramSocket {
    type Addr = SocketAddr;

    fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        if self.fail_recv.replace(false) {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionReset,
                "mock: recv failed",
            ));
        }
        let (data, from) = self
            .inbound
            .borrow_mut()
            .pop_front()
            .ok_or_else(|| io::Error::new(io::ErrorKind::WouldBlock, "no queued datagram"))?;
        let n = data.len().min(buf.len());
        buf[..n].copy_from_slice(&data[..n]);
        Ok((n, from))
    }

    fn send_to(&self, buf: &[u8], addr: &SocketAddr) -> io::Result<usize> {
        self.sent.borrow_mut().push((buf.to_vec(), *addr));
        Ok(buf.len())
    }
}

/// A test-only `Read + Write` byte stream backed by in-memory buffers — exercise [`MessageStream`]
/// code in unit tests with no real socket. Enabled by the **`mock`** feature (put it in your
/// `[dev-dependencies]`). Queue inbound bytes with [`push_inbound`](Self::push_inbound) and inspect
/// what was written with [`written`](Self::written).
///
/// Unlike `std::io::Cursor` it keeps **separate** read and write buffers (so it handles duplex
/// request/reply cleanly), and it can deliver inbound bytes a few at a time
/// ([`with_chunk_size`](Self::with_chunk_size)) — to exercise `read_message`'s buffer-more-and-retry
/// loop, i.e. a message split across reads, which `Cursor` (one read = everything) cannot.
///
/// ```
/// use bnb::{bin, MessageStream, MockStream};
/// #[bin(big)]
/// #[derive(Debug, PartialEq, Eq)]
/// struct Ping {
///     seq: u16,
/// }
///
/// // deliver the 2-byte Ping one byte per read — forces the read-more loop
/// let mut conn = MessageStream::new(MockStream::with_chunk_size(1));
/// conn.get_mut().push_inbound(&Ping { seq: 7 }.to_bytes().unwrap());
///
/// let ping: Ping = conn.read_message().unwrap();
/// assert_eq!(ping, Ping { seq: 7 });
/// conn.write_message(&Ping { seq: 8 }).unwrap();
/// assert_eq!(conn.get_mut().written(), &Ping { seq: 8 }.to_bytes().unwrap()[..]);
/// ```
#[cfg(feature = "mock")]
#[derive(Debug, Default, Clone)]
pub struct MockStream {
    inbound: VecDeque<u8>,
    outbound: Vec<u8>,
    chunk: usize,
    fail_after: Option<usize>,
    read_total: usize,
}

#[cfg(feature = "mock")]
impl MockStream {
    /// An empty stream that delivers all available inbound bytes per `read`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Like [`new`](Self::new), but each `read` returns at most `n` bytes (`n > 0`) — to simulate a
    /// stream that dribbles one message across several reads (the `Incomplete` / read-more path).
    #[must_use]
    pub fn with_chunk_size(n: usize) -> Self {
        Self {
            chunk: n,
            ..Self::default()
        }
    }

    /// Queue bytes to be returned by future `read`s.
    pub fn push_inbound(&mut self, bytes: &[u8]) {
        self.inbound.extend(bytes.iter().copied());
    }

    /// All bytes written to the stream so far.
    #[must_use]
    pub fn written(&self) -> &[u8] {
        &self.outbound
    }

    /// After `n` inbound bytes have been read, every further `read` fails with `ConnectionReset`
    /// — to test a connection that drops mid-message (the error surfaces through `read_message`).
    #[must_use]
    pub fn fail_after(mut self, n: usize) -> Self {
        self.fail_after = Some(n);
        self
    }
}

#[cfg(feature = "mock")]
impl Read for MockStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Some(at) = self.fail_after {
            if self.read_total >= at {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionReset,
                    "mock: connection reset",
                ));
            }
        }
        if self.inbound.is_empty() || buf.is_empty() {
            return Ok(0); // EOF: no more inbound (as a closed connection would read)
        }
        let mut cap = if self.chunk == 0 {
            buf.len()
        } else {
            buf.len().min(self.chunk)
        };
        cap = cap.min(self.inbound.len());
        if let Some(at) = self.fail_after {
            cap = cap.min(at - self.read_total); // stop exactly at the failure point
        }
        for slot in buf.iter_mut().take(cap) {
            *slot = self.inbound.pop_front().unwrap();
        }
        self.read_total += cap;
        Ok(cap)
    }
}

#[cfg(feature = "mock")]
impl Write for MockStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.outbound.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(all(test, feature = "mock"))]
mod component {
    //! Component tests: the `net` wrappers driven by the in-memory mocks, one call at a time
    //! (a queued read, a captured write, chunked reassembly, error injection, the accessors).
    use super::{MessageReadError, read_message};
    use std::cell::Cell;

    thread_local! { static ATTEMPTS: Cell<usize> = const { Cell::new(0) }; }

    fn probe<S: crate::Source>(source: &mut S) -> Result<u32, BitError> {
        ATTEMPTS.set(ATTEMPTS.get() + 1);
        let mode = source.read_bits(8)?;
        if mode == 3 {
            return Err(BitError::new(
                ErrorKind::Incomplete {
                    needed: Some(usize::MAX),
                },
                source.bit_pos(),
            )
            .in_field("body"));
        }
        source
            .read_bits(32)
            .map(|value| u32::try_from(value).unwrap())
            .map_err(|mut error| {
                if error.is_incomplete() {
                    match mode {
                        0 => error.kind = ErrorKind::Incomplete { needed: None },
                        1 => error.kind = ErrorKind::Incomplete { needed: Some(0) },
                        _ => {}
                    }
                }
                error.in_field("body")
            })
    }

    #[allow(clippy::trivially_copy_pass_by_ref)] // Field-codec write callback takes a borrow.
    fn write_probe<K: crate::Sink>(value: &u32, sink: &mut K) -> Result<(), BitError> {
        sink.write_bits(2, 8)?;
        sink.write_bits(u128::from(*value), 32)
    }

    #[bin(codec(parse = probe, write = write_probe))]
    #[derive(Debug, PartialEq, Eq)]
    struct Probe(u32);

    #[bin(big)]
    #[derive(Debug)]
    struct Absolute {
        #[br(seek = 8, assert(tag == b'A'))]
        tag: u8,
        body: u32,
    }

    #[test]
    fn compaction_invalidates_a_hint_before_reading_past_a_new_terminal_error() {
        let mut buffer = BitBuf::bounded(8);
        buffer.push(&[0, b'A']).unwrap();
        buffer.try_pull::<u8>().unwrap();
        assert_eq!(
            buffer.try_pull::<Absolute>().unwrap_err().kind,
            ErrorKind::Incomplete { needed: Some(4) }
        );
        let mut reader = &b"X123"[..];
        let result = read_message::<Absolute, _>(&mut reader, &mut buffer, &mut [0; 1]);
        assert!(matches!(
            result,
            Err(MessageReadError::Codec(BitError {
                kind: ErrorKind::Convert { .. },
                field: Some("tag"),
                ..
            }))
        ));
        assert_eq!(
            reader, b"123",
            "rebase must reveal the error before consuming any more input"
        );
        assert_eq!(buffer.try_pull::<u16>().unwrap(), 0x4158);
    }

    #[bin(codec(parse = crate::codecs::cstring::parse_utf8, write = crate::codecs::cstring::write_utf8))]
    #[derive(Debug, PartialEq, Eq)]
    struct Delimited(String);

    #[test]
    fn delimiter_messages_and_raw_tail_survive_arbitrary_read_chunks() {
        for chunk in 1..=16 {
            let mut inner = MockStream::with_chunk_size(chunk);
            inner.push_inbound(b"first\0second\0raw");
            let mut stream = MessageStream::bounded(inner, 32);
            assert_eq!(stream.read_message::<Delimited>().unwrap().0, "first");
            assert_eq!(stream.read_message::<Delimited>().unwrap().0, "second");
            let mut tail = Vec::new();
            stream.read_to_end(&mut tail).unwrap();
            assert_eq!(tail, b"raw");
        }
    }

    #[test]
    fn positive_hints_batch_attempts_unknown_and_zero_retry_after_each_read() {
        for (mode, attempts) in [(0, 4), (1, 4), (2, 2)] {
            ATTEMPTS.set(0);
            let mut buffer = BitBuf::bounded(8);
            buffer.push(&[mode, 0x11]).unwrap();
            let mut reader = MockStream::with_chunk_size(1);
            reader.push_inbound(&[0x22, 0x33, 0x44]);
            let result = read_message::<Probe, _>(&mut reader, &mut buffer, &mut [0; 8]).unwrap();
            assert_eq!(result, Probe(0x1122_3344), "mode {mode}");
            assert_eq!(ATTEMPTS.get(), attempts, "mode {mode}");
            assert!(buffer.is_empty());
        }
    }

    #[test]
    fn empty_scratch_only_errors_when_buffered_decode_needs_refill() {
        let mut buffer = BitBuf::new();
        buffer.push(&[0x12, 0x34, 0x56]).unwrap();
        let mut reader = Scripted {
            bytes: io::Cursor::new(vec![]),
            reads: 0,
            fail: None,
        };
        assert_eq!(
            read_message::<u16, _>(&mut reader, &mut buffer, &mut []).unwrap(),
            0x1234
        );
        assert!(
            matches!(read_message::<u16, _>(&mut reader, &mut buffer, &mut []),
            Err(MessageReadError::Io(error)) if error.kind() == io::ErrorKind::InvalidInput)
        );
        assert_eq!(reader.reads, 0);
        assert_eq!(buffer.try_pull::<u8>().unwrap(), 0x56);
    }

    #[test]
    fn huge_hint_fills_capacity_without_an_extra_read_or_losing_input() {
        let mut buffer = BitBuf::bounded(4);
        buffer.push(&[3]).unwrap();
        let mut reader = Scripted {
            bytes: io::Cursor::new(vec![1, 2, 3, 4]),
            reads: 0,
            fail: None,
        };
        assert!(matches!(
            read_message::<Probe, _>(&mut reader, &mut buffer, &mut [0; 8]),
            Err(MessageReadError::Codec(BitError {
                kind: ErrorKind::BufferFull { cap: 4 },
                ..
            }))
        ));
        assert_eq!(reader.reads, 1);
        assert_eq!(reader.bytes.position(), 3);
        assert_eq!(buffer.try_pull::<u32>().unwrap(), 0x0301_0203);
        assert_eq!(reader.bytes.get_ref()[3..], [4]);
    }

    #[test]
    fn overshooting_a_hint_retries_decode_without_another_transport_read() {
        let mut buffer = BitBuf::bounded(8);
        buffer.push(&[2, 0x11]).unwrap();
        let mut reader = Scripted {
            bytes: io::Cursor::new(vec![0x22, 0x33, 0x44, 0x99]),
            reads: 0,
            fail: None,
        };
        assert_eq!(
            read_message::<Probe, _>(&mut reader, &mut buffer, &mut [0; 8]).unwrap(),
            Probe(0x1122_3344)
        );
        assert_eq!(reader.reads, 1);
        assert_eq!(buffer.try_pull::<u8>().unwrap(), 0x99);
    }

    #[test]
    fn physical_eof_ignores_outstanding_hint_and_retains_typed_context() {
        let mut buffer = BitBuf::new();
        buffer.push(&[3]).unwrap();
        let mut reader = &b""[..];
        let error = read_message::<Probe, _>(&mut reader, &mut buffer, &mut [0; 8]).unwrap_err();
        assert!(matches!(
            error,
            MessageReadError::Codec(BitError {
                kind: ErrorKind::IncompleteAtEof {
                    needed: Some(usize::MAX)
                },
                at: 8,
                field: Some("body")
            })
        ));
        assert_eq!(buffer.try_pull::<u8>().unwrap(), 3);
    }

    #[bin(big)]
    #[derive(Debug, PartialEq, Eq)]
    enum EofFallback {
        #[bin(magic = b"ABCDE")]
        Long,
        Raw(crate::u4),
    }

    #[test]
    fn finite_fallback_success_also_consumes_final_byte_padding() {
        let mut buffer = BitBuf::new();
        buffer.push(b"A").unwrap();
        assert_eq!(
            read_message::<EofFallback, _>(&mut &b""[..], &mut buffer, &mut [0; 8]).unwrap(),
            EofFallback::Raw(crate::u4::new(4))
        );
        assert!(buffer.is_empty());
    }

    #[derive(Debug, thiserror::Error)]
    #[error("transport marker {0}")]
    struct Marker(u64);

    #[test]
    fn transport_error_retains_its_original_owned_source() {
        struct Fails(Option<io::Error>);
        impl Read for Fails {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(self.0.take().unwrap())
            }
        }
        let original = io::Error::new(io::ErrorKind::ConnectionReset, Marker(0x1234));
        let identity = std::ptr::from_ref(
            original
                .get_ref()
                .unwrap()
                .downcast_ref::<Marker>()
                .unwrap(),
        );
        let mut reader = Fails(Some(original));
        let error =
            read_message::<u16, _>(&mut reader, &mut BitBuf::new(), &mut [0; 8]).unwrap_err();
        assert!(
            std::error::Error::source(&error)
                .unwrap()
                .downcast_ref::<io::Error>()
                .is_some()
        );
        let MessageReadError::Io(error) = error else {
            panic!("transport failure must stay typed")
        };
        assert_eq!(error.kind(), io::ErrorKind::ConnectionReset);
        assert_eq!(
            std::ptr::from_ref(error.get_ref().unwrap().downcast_ref::<Marker>().unwrap()),
            identity
        );
    }

    #[cfg(feature = "tokio-io")]
    mod asynchronous {
        use super::*;
        use crate::net::read_message_async;
        use std::{
            future::Future,
            pin::Pin,
            task::{Context, Poll, Waker},
        };
        use tokio::io::{AsyncRead, ReadBuf};

        // A pending read neither touches the destination nor consumes transport bytes.
        struct Pausable {
            reader: Scripted,
            chunk: usize,
            pause_after: Option<u64>,
        }
        impl AsyncRead for Pausable {
            fn poll_read(
                mut self: Pin<&mut Self>,
                _: &mut Context<'_>,
                out: &mut ReadBuf<'_>,
            ) -> Poll<io::Result<()>> {
                if self.pause_after == Some(self.reader.bytes.position()) {
                    return Poll::Pending;
                }
                let limit = out.remaining().min(self.chunk);
                let count = self.reader.read(&mut out.initialize_unfilled()[..limit])?;
                out.advance(count);
                Poll::Ready(Ok(()))
            }
        }

        #[tokio::test]
        async fn async_compaction_invalidates_outstanding_hint_without_overreading() {
            let mut buffer = BitBuf::bounded(8);
            buffer.push(&[0, b'A']).unwrap();
            buffer.try_pull::<u8>().unwrap();
            let mut reader = &b"X123"[..];
            let result =
                read_message_async::<Absolute, _>(&mut reader, &mut buffer, &mut [0; 1]).await;
            assert!(matches!(
                result,
                Err(MessageReadError::Codec(BitError {
                    kind: ErrorKind::Convert { .. },
                    field: Some("tag"),
                    ..
                }))
            ));
            assert_eq!(reader, b"123");
            assert_eq!(buffer.try_pull::<u16>().unwrap(), 0x4158);
        }

        #[tokio::test]
        async fn async_hints_match_sync_including_interruption_and_overshoot() {
            for mode in [0, 1, 2] {
                for chunk in [1, 2, 8] {
                    let mut buffer = BitBuf::bounded(8);
                    buffer.push(&[mode, 0x11]).unwrap();
                    let mut reader = Pausable {
                        reader: Scripted {
                            bytes: io::Cursor::new(vec![0x22, 0x33, 0x44, 0x99]),
                            reads: 0,
                            fail: Some(io::ErrorKind::Interrupted),
                        },
                        chunk,
                        pause_after: None,
                    };
                    ATTEMPTS.set(0);
                    assert_eq!(
                        read_message_async::<Probe, _>(&mut reader, &mut buffer, &mut [0; 8])
                            .await
                            .unwrap(),
                        Probe(0x1122_3344)
                    );
                    if mode == 2 {
                        assert_eq!(ATTEMPTS.get(), 2);
                    }
                    if chunk == 8 {
                        assert_eq!(
                            reader.reader.reads, 2,
                            "one Interrupted plus one successful read; no extra I/O"
                        );
                    }
                    assert_eq!(
                        read_message_async::<u8, _>(&mut reader, &mut buffer, &mut [0; 8])
                            .await
                            .unwrap(),
                        0x99
                    );
                }
            }
        }

        #[tokio::test]
        async fn cancellation_keeps_every_completed_read_and_retry_decodes_once() {
            for pause_after in 0..5 {
                let mut reader = Pausable {
                    reader: Scripted {
                        bytes: io::Cursor::new(vec![2, 0x11, 0x22, 0x33, 0x44, 0x99]),
                        reads: 0,
                        fail: None,
                    },
                    chunk: 1,
                    pause_after: Some(pause_after),
                };
                let mut buffer = BitBuf::bounded(8);
                let mut scratch = [0; 8];
                {
                    let future =
                        read_message_async::<Probe, _>(&mut reader, &mut buffer, &mut scratch);
                    let mut future = std::pin::pin!(future);
                    let mut context = Context::from_waker(Waker::noop());
                    assert!(future.as_mut().poll(&mut context).is_pending());
                }
                assert_eq!(buffer.bit_len(), usize::try_from(pause_after).unwrap() * 8);
                assert_eq!(reader.reader.bytes.position(), pause_after);
                reader.pause_after = None;
                assert_eq!(
                    read_message_async::<Probe, _>(&mut reader, &mut buffer, &mut scratch)
                        .await
                        .unwrap(),
                    Probe(0x1122_3344)
                );
                assert_eq!(
                    read_message_async::<u8, _>(&mut reader, &mut buffer, &mut scratch)
                        .await
                        .unwrap(),
                    0x99
                );
                assert!(buffer.is_empty());
            }
        }

        #[tokio::test]
        async fn async_capacity_scratch_and_finite_errors_preserve_input() {
            let mut buffer = BitBuf::bounded(4);
            buffer.push(&[3]).unwrap();
            let mut reader = &b"abcdef"[..];
            assert!(matches!(
                read_message_async::<Probe, _>(&mut reader, &mut buffer, &mut [0; 8]).await,
                Err(MessageReadError::Codec(BitError {
                    kind: ErrorKind::BufferFull { cap: 4 },
                    ..
                }))
            ));
            assert_eq!(reader, b"def");
            assert_eq!(buffer.try_pull::<u32>().unwrap(), 0x0361_6263);
            buffer.push(&[2, 0x11]).unwrap();
            assert!(
                matches!(read_message_async::<Probe, _>(&mut reader, &mut buffer, &mut []).await,
                Err(MessageReadError::Io(error)) if error.kind() == io::ErrorKind::InvalidInput)
            );
            let mut reader = &b""[..];
            assert!(matches!(
                read_message_async::<Probe, _>(&mut reader, &mut buffer, &mut [0; 8]).await,
                Err(MessageReadError::Codec(BitError {
                    kind: ErrorKind::UnexpectedEof { .. },
                    field: Some("body"),
                    ..
                }))
            ));
            assert_eq!(buffer.try_pull::<u16>().unwrap(), 0x0211);
            assert!(
                matches!(read_message_async::<u8, _>(&mut reader, &mut buffer, &mut [0; 8]).await,
                Err(MessageReadError::Io(error)) if error.kind() == io::ErrorKind::UnexpectedEof)
            );
            buffer.push(b"A").unwrap();
            assert_eq!(
                read_message_async::<EofFallback, _>(&mut reader, &mut buffer, &mut [])
                    .await
                    .unwrap_err()
                    .to_string(),
                "message read failed: message read scratch is empty"
            );
            assert_eq!(
                read_message_async::<EofFallback, _>(&mut reader, &mut buffer, &mut [0; 8])
                    .await
                    .unwrap(),
                EofFallback::Raw(crate::u4::new(4))
            );
            assert!(buffer.is_empty());
        }

        #[tokio::test]
        async fn async_io_failure_preserves_prefix_for_retry_and_delimited_tail() {
            let mut buffer = BitBuf::bounded(32);
            buffer.push(b"fir").unwrap();
            let mut reader = Pausable {
                reader: Scripted {
                    bytes: io::Cursor::new(b"st\0second\0".to_vec()),
                    reads: 0,
                    fail: Some(io::ErrorKind::WouldBlock),
                },
                chunk: 1,
                pause_after: None,
            };
            let mut scratch = [0; 16];
            assert!(
                matches!(read_message_async::<Delimited, _>(&mut reader, &mut buffer, &mut scratch).await,
                Err(MessageReadError::Io(error)) if error.kind() == io::ErrorKind::WouldBlock)
            );
            assert_eq!(buffer.bit_len(), 24);
            assert_eq!(
                read_message_async::<Delimited, _>(&mut reader, &mut buffer, &mut scratch)
                    .await
                    .unwrap()
                    .0,
                "first"
            );
            assert_eq!(
                read_message_async::<Delimited, _>(&mut reader, &mut buffer, &mut scratch)
                    .await
                    .unwrap()
                    .0,
                "second"
            );
        }
    }
    use crate::{BitBuf, BitError, ErrorKind};
    use bnb::{MessageDatagram, MessageStream, MockDatagramSocket, MockStream, bin};
    use std::io::{self, Read, Write};

    #[derive(Debug)]
    struct Scripted {
        bytes: io::Cursor<Vec<u8>>,
        reads: usize,
        fail: Option<io::ErrorKind>,
    }
    impl Read for Scripted {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            self.reads += 1;
            if let Some(kind) = self.fail.take() {
                return Err(kind.into());
            }
            self.bytes.read(out)
        }
    }

    #[test]
    fn full_buffer_never_reads_and_parts_recover_every_byte() {
        let inner = Scripted {
            bytes: io::Cursor::new(vec![1, 2, 3, 4, 5]),
            reads: 0,
            fail: None,
        };
        let mut stream = MessageStream::bounded(inner, 2);
        assert!(matches!(
            stream.read_message::<u32>().unwrap_err(),
            MessageReadError::Codec(BitError {
                kind: ErrorKind::BufferFull { cap: 2 },
                ..
            })
        ));
        assert_eq!(stream.get_ref().reads, 1);
        assert_eq!(stream.get_ref().bytes.position(), 2);
        assert!(stream.read_message::<u32>().is_err());
        assert_eq!(stream.get_ref().reads, 1);
        let stream = stream.try_into_inner().unwrap_err();
        let (inner, mut buffer) = stream.into_parts();
        buffer.grow(2);
        let mut stream = MessageStream::from_parts(inner, buffer);
        assert_eq!(stream.read_message::<u32>().unwrap(), 0x0102_0304);
        let mut tail = [0; 1];
        stream.read_exact(&mut tail).unwrap();
        assert_eq!(tail, [5]);
        assert!(stream.try_into_inner().is_ok());
    }

    #[test]
    fn raw_read_drains_retained_input_without_an_additional_read() {
        let inner = Scripted {
            bytes: io::Cursor::new(vec![1, 2, 3]),
            reads: 0,
            fail: None,
        };
        let mut stream = MessageStream::new(inner);
        assert_eq!(stream.read_message::<u8>().unwrap(), 1);
        let mut out = [0; 8];
        assert_eq!(stream.read(&mut []).unwrap(), 0);
        assert_eq!(stream.read(&mut out).unwrap(), 2);
        assert_eq!(&out[..2], &[2, 3]);
        assert_eq!(stream.get_ref().reads, 1);
        assert_eq!(stream.read(&mut out).unwrap(), 0);
    }

    #[test]
    fn bounded_read_reclaims_consumed_prefix_before_the_next_message() {
        let inner = Scripted {
            bytes: io::Cursor::new(vec![1, 2, 3, 4, 5, 6]),
            reads: 0,
            fail: None,
        };
        let mut stream = MessageStream::bounded(inner, 4);
        assert_eq!(stream.read_message::<u16>().unwrap(), 0x0102);
        assert_eq!(stream.read_message::<u32>().unwrap(), 0x0304_0506);
        assert_eq!(stream.get_ref().reads, 2);
        assert_eq!(stream.get_ref().bytes.position(), 6);
    }

    #[test]
    fn raw_read_rejects_unaligned_parts_without_losing_bits() {
        let mut buffer = BitBuf::new();
        buffer.push(&[0xab]).unwrap();
        buffer.try_pull::<crate::u4>().unwrap();
        let mut stream = MessageStream::from_parts(&b"tail"[..], buffer);
        assert_eq!(stream.read(&mut []).unwrap(), 0);
        let error = BitError::from(stream.read(&mut [0; 1]).unwrap_err());
        assert_eq!(error.kind, ErrorKind::NotByteAligned);
        let (inner, mut buffer) = stream.into_parts();
        assert_eq!(inner, b"tail");
        assert_eq!(buffer.try_pull::<crate::u4>().unwrap().value(), 0xb);
    }

    #[test]
    fn subbyte_messages_are_independently_padded_before_raw_handoff() {
        for short_first in [false, true] {
            let mut sender = MessageStream::new(Vec::new());
            if short_first {
                sender.write_message(&crate::u4::new(0xa)).unwrap();
            }
            sender.write_message(&crate::u12::new(0xbcd)).unwrap();
            if !short_first {
                sender.write_message(&crate::u4::new(0xa)).unwrap();
            }
            sender.write_all(b"raw").unwrap();
            sender.flush().unwrap();
            let wire = sender.try_into_inner().unwrap();
            let mut receiver = MessageStream::new(&wire[..]);
            if short_first {
                assert_eq!(receiver.read_message::<crate::u4>().unwrap().value(), 0xa);
            }
            assert_eq!(
                receiver.read_message::<crate::u12>().unwrap().value(),
                0xbcd
            );
            if !short_first {
                assert_eq!(receiver.read_message::<crate::u4>().unwrap().value(), 0xa);
            }
            let mut tail = [0; 3];
            receiver.read_exact(&mut tail).unwrap();
            assert_eq!(&tail, b"raw");
            assert_eq!(receiver.read(&mut tail).unwrap(), 0);
        }
        let mut receiver = MessageStream::new(&[0xaf, 0x99][..]);
        assert_eq!(receiver.read_message::<crate::u4>().unwrap().value(), 0xa);
        assert_eq!(receiver.read_message::<u8>().unwrap(), 0x99);
    }

    #[test]
    fn transport_errors_and_truncated_eof_keep_accepted_bytes() {
        let mut buffer = BitBuf::new();
        buffer.push(&[1]).unwrap();
        let inner = Scripted {
            bytes: io::Cursor::new(vec![2]),
            reads: 0,
            fail: Some(io::ErrorKind::WouldBlock),
        };
        let mut stream = MessageStream::from_parts(inner, buffer);
        assert!(matches!(stream.read_message::<u16>().unwrap_err(),
            MessageReadError::Io(error) if error.kind() == io::ErrorKind::WouldBlock));
        assert_eq!(stream.read_message::<u16>().unwrap(), 0x0102);
        assert!(matches!(stream.read_message::<u16>().unwrap_err(),
            MessageReadError::Io(error) if error.kind() == io::ErrorKind::UnexpectedEof));
        let mut stream = MessageStream::new(&[1][..]);
        assert!(matches!(
            stream.read_message::<u16>().unwrap_err(),
            MessageReadError::Codec(BitError {
                kind: ErrorKind::UnexpectedEof {
                    needed: 16,
                    remaining: 8
                },
                ..
            })
        ));
        assert_eq!(stream.read_message::<u8>().unwrap(), 1);
    }

    #[test]
    fn interrupted_read_retries_without_losing_the_retained_prefix() {
        let mut buffer = BitBuf::bounded(2);
        buffer.push(&[0x12]).unwrap();
        let inner = Scripted {
            bytes: io::Cursor::new(vec![0x34]),
            reads: 0,
            fail: Some(io::ErrorKind::Interrupted),
        };
        let mut stream = MessageStream::from_parts(inner, buffer);
        assert_eq!(stream.read_message::<u16>().unwrap(), 0x1234);
        assert_eq!(stream.get_ref().reads, 2);
        assert!(stream.try_into_inner().is_ok());
    }

    #[test]
    fn datagram_rejects_trailing_whole_bytes_but_accepts_final_padding() {
        let mut peer = MessageDatagram::new(MockDatagramSocket::new());
        let from = "127.0.0.1:5000".parse().unwrap();
        peer.get_ref().push_inbound(&[0xaf], from);
        assert_eq!(peer.recv_message::<crate::u4>().unwrap().0.value(), 0xa);
        peer.get_ref().push_inbound(&[0xa0, 0xff], from);
        assert_eq!(
            peer.recv_message::<crate::u4>().unwrap_err().kind,
            ErrorKind::TrailingBytes { remaining: 1 }
        );
    }

    #[bin(big)]
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Msg {
        seq: u16,
    }

    // --- MessageStream over MockStream -------------------------------------------------

    #[test]
    fn mock_stream_honors_its_scripted_chunk_bound() {
        let mut stream = MockStream::with_chunk_size(2);
        stream.push_inbound(&[1, 2, 3, 4]);
        let mut bytes = [0; 4];
        assert_eq!(stream.read(&mut bytes).unwrap(), 2);
        assert_eq!(&bytes[..2], &[1, 2]);
        assert_eq!(stream.read(&mut bytes).unwrap(), 2);
        assert_eq!(&bytes[..2], &[3, 4]);
    }

    #[test]
    fn stream_write_message_is_captured() {
        let mut conn = MessageStream::new(MockStream::new());
        conn.write_message(&Msg { seq: 7 }).unwrap();
        assert_eq!(
            conn.get_mut().written(),
            &Msg { seq: 7 }.to_bytes().unwrap()[..]
        );
    }

    #[test]
    fn stream_reads_a_queued_message() {
        let mut conn = MessageStream::new(MockStream::new());
        conn.get_mut()
            .push_inbound(&Msg { seq: 0xABCD }.to_bytes().unwrap());
        assert_eq!(conn.read_message::<Msg>().unwrap(), Msg { seq: 0xABCD });
    }

    #[test]
    fn stream_reassembles_a_message_split_across_reads() {
        // One byte per read forces the buffer-more-and-retry loop in read_message.
        let mut conn = MessageStream::new(MockStream::with_chunk_size(1));
        conn.get_mut()
            .push_inbound(&Msg { seq: 0x1234 }.to_bytes().unwrap());
        assert_eq!(conn.read_message::<Msg>().unwrap(), Msg { seq: 0x1234 });
    }

    #[test]
    fn stream_eof_mid_message_is_an_error() {
        // Only one of the two needed bytes is available, then the connection closes.
        let mut conn = MessageStream::new(MockStream::new());
        conn.get_mut().push_inbound(&[0x12]);
        assert!(conn.read_message::<Msg>().is_err());
    }

    #[test]
    fn stream_connection_reset_surfaces_as_an_error() {
        let mut conn = MessageStream::new(MockStream::new().fail_after(0));
        conn.get_mut()
            .push_inbound(&Msg { seq: 1 }.to_bytes().unwrap());
        assert!(conn.read_message::<Msg>().is_err());
    }

    #[test]
    fn stream_into_inner_recovers_the_transport() {
        let conn = MessageStream::new(MockStream::new());
        let _inner: MockStream = conn.try_into_inner().unwrap();
    }

    // --- MessageDatagram over MockDatagramSocket ---------------------------------------

    #[test]
    fn datagram_recv_then_send_to_the_sender() {
        let mut peer = MessageDatagram::new(MockDatagramSocket::new());
        let from = "127.0.0.1:5000".parse().unwrap();
        peer.get_ref()
            .push_inbound(&Msg { seq: 7 }.to_bytes().unwrap(), from);

        let (msg, who) = peer.recv_message::<Msg>().unwrap();
        assert_eq!(msg, Msg { seq: 7 });
        assert_eq!(who, from);

        let n = peer.send_message(&Msg { seq: 8 }, &who).unwrap();
        assert_eq!(n, 2, "send_message returns the byte count");
        assert_eq!(
            peer.get_ref().sent()[0].0,
            Msg { seq: 8 }.to_bytes().unwrap()
        );
        assert_eq!(
            peer.get_ref().sent()[0].1,
            who,
            "sent to the original sender"
        );
    }

    #[test]
    fn datagram_recv_error_is_injected() {
        let mut peer = MessageDatagram::new(MockDatagramSocket::new().fail_next_recv());
        assert!(peer.recv_message::<Msg>().is_err());
    }

    #[test]
    fn datagram_recv_malformed_is_a_codec_error() {
        #[bin(big, magic = 0xCAFEu16)]
        #[derive(Debug, PartialEq, Eq)]
        struct M {
            v: u8,
        }
        let mut peer = MessageDatagram::new(MockDatagramSocket::new());
        let from = "127.0.0.1:1".parse().unwrap();
        peer.get_ref().push_inbound(&[0x00, 0x00, 0x09], from); // wrong magic
        assert!(peer.recv_message::<M>().is_err());
    }

    #[test]
    fn datagram_with_capacity_truncates_an_oversized_datagram() {
        // Capacity 2 → only the first two bytes are delivered (OS-style truncation).
        let mut peer = MessageDatagram::with_capacity(MockDatagramSocket::new(), 2);
        let from = "127.0.0.1:2".parse().unwrap();
        peer.get_ref().push_inbound(&[0x00, 0x05, 0xFF, 0xFF], from);
        let (msg, _) = peer.recv_message::<Msg>().unwrap();
        assert_eq!(msg, Msg { seq: 5 });
    }

    #[test]
    fn datagram_get_mut_and_into_inner() {
        let mut peer = MessageDatagram::new(MockDatagramSocket::new());
        let _m: &mut MockDatagramSocket = peer.get_mut();
        let _inner: MockDatagramSocket = peer.into_inner();
    }
}
