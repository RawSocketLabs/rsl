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
//! feature, [`MockDatagramSocket`]; the trait is *sealed*, so those are the only impls). Both
//! wrappers bridge `std::io::Error` into [`BitError`] (the `std` feature), so a single `?` covers
//! I/O *and* codec errors.

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
    /// (via [`BitBuf::pull`]). Messages are independently byte-padded, like `write_message`:
    /// the unused bits of the last byte are accepted and consumed. Use `BitBuf` directly
    /// for bit-packed concatenation. Decode callbacks may run repeatedly as input arrives.
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
    pub fn read_message<T: BitDecode + BitEncode>(&mut self) -> Result<T, BitError> {
        loop {
            // `pull` decodes in `T`'s own layout, returns `None` until a whole message is
            // buffered, and reclaims consumed bytes — the framing logic lives in `BitBuf`.
            if let Some(msg) = self.buf.pull::<T>()? {
                self.buf.finish_byte();
                return Ok(msg);
            }
            let mut chunk = [0u8; 4096];
            let available = self
                .buf
                .read_capacity()
                .unwrap_or(chunk.len())
                .min(chunk.len());
            if available == 0 {
                return Err(BitError::new(
                    ErrorKind::BufferFull {
                        cap: self.buf.capacity().expect("bounded buffer"),
                    },
                    crate::Source::bit_pos(&self.buf),
                ));
            }
            let n = match self.inner.read(&mut chunk[..available]) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                result => result?,
            };
            if n == 0 {
                if let Some(msg) = self.buf.pull_eof::<T>()? {
                    self.buf.finish_byte();
                    return Ok(msg);
                }
                return Err(
                    io::Error::new(io::ErrorKind::UnexpectedEof, "connection closed").into(),
                );
            }
            // Read was limited to the exact free retained capacity, before touching the stream.
            self.buf
                .push(&chunk[..n])
                .expect("read cannot exceed reserved input capacity");
        }
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
/// — under the `mock` feature — a [`MockDatagramSocket`]). It owns the socket and reuses one
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
            stream.read_message::<u32>().unwrap_err().kind,
            ErrorKind::BufferFull { cap: 2 }
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
        assert_eq!(
            stream.read_message::<u16>().unwrap_err().kind,
            ErrorKind::Io(io::ErrorKind::WouldBlock)
        );
        assert_eq!(stream.read_message::<u16>().unwrap(), 0x0102);
        assert_eq!(
            stream.read_message::<u16>().unwrap_err().kind,
            ErrorKind::Io(io::ErrorKind::UnexpectedEof)
        );
        let mut stream = MessageStream::new(&[1][..]);
        assert_eq!(
            stream.read_message::<u16>().unwrap_err().kind,
            ErrorKind::UnexpectedEof {
                needed: 16,
                remaining: 8
            }
        );
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
