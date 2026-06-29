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

use crate::{BitBuf, BitDecode, BitEncode, BitError, BitReader, BitWriter};
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
        // Decode in `T`'s own byte/bit order (not the reader's default).
        let mut r = BitReader::with_layout(&self.buf[..n], <T as BitEncode>::LAYOUT);
        let msg = <T as BitDecode>::bit_decode(&mut r)?;
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
    use bnb::{MessageDatagram, MessageStream, MockDatagramSocket, MockStream, bin};

    #[bin(big)]
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Msg {
        seq: u16,
    }

    // --- MessageStream over MockStream -------------------------------------------------

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
        let _inner: MockStream = conn.into_inner();
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
