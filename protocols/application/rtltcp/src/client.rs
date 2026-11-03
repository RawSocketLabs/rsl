//! A blocking `rtl_tcp` client on std's [`TcpStream`](std::net::TcpStream).
//!
//! The client owns the socket: it reads and checks the greeting, sends commands, and reads the
//! raw interleaved CU8 sample stream into caller buffers. It applies no device policy (gain,
//! settle windows, sample conversion); that belongs to the caller.

use std::io::{self, ErrorKind, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use thiserror::Error;

use crate::{Command, Greeting, Opcode};

/// Chunk size used when discarding stream bytes (the workspace's stack-array ceiling).
const DISCARD_CHUNK: usize = 16 * 1024;

/// An error from the [`Client`].
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum ClientError {
    /// The address could not be resolved.
    #[error("could not resolve address: {0}")]
    Resolve(#[source] io::Error),
    /// The address resolved to no socket addresses.
    #[error("address resolved to no socket addresses")]
    NoAddress,
    /// The TCP connection could not be opened or configured.
    #[error("could not connect: {0}")]
    Connect(#[source] io::Error),
    /// The server sent no complete greeting.
    #[error("no greeting from server: {0}")]
    Handshake(#[source] io::Error),
    /// The server's greeting magic is not `RTL0`.
    #[error("not an rtl_tcp server (expected magic RTL0, got {magic:02x?})")]
    NotRtlTcp {
        /// The magic the server sent.
        magic: [u8; 4],
    },
    /// A command could not be sent.
    #[error("command {opcode:?} failed: {source}")]
    Command {
        /// The opcode being sent.
        opcode: Opcode,
        /// The transport error.
        #[source]
        source: io::Error,
    },
    /// Discarding stream bytes failed.
    #[error("drain failed: {0}")]
    Drain(#[source] io::Error),
    /// Reading stream bytes failed.
    #[error("read failed: {0}")]
    Read(#[source] io::Error),
    /// The server closed the connection.
    #[error("server closed the connection")]
    Closed,
    /// A wire encode/decode error from the `bnb` codec.
    #[error("codec error: {0}")]
    Codec(#[from] bnb::BitError),
}

/// A connected `rtl_tcp` client.
#[derive(Debug)]
pub struct Client {
    stream: TcpStream,
    greeting: Greeting,
}

impl Client {
    /// Connect to `addr` (the first address it resolves to) within `timeout`, set
    /// `TCP_NODELAY`, and read the greeting.
    ///
    /// # Errors
    ///
    /// [`ClientError::Resolve`] / [`ClientError::NoAddress`] when `addr` does not resolve,
    /// [`ClientError::Connect`] when the connection fails, and the errors of
    /// [`Client::from_stream`].
    pub fn connect<A: ToSocketAddrs>(addr: A, timeout: Duration) -> Result<Self, ClientError> {
        let resolved = addr
            .to_socket_addrs()
            .map_err(ClientError::Resolve)?
            .next()
            .ok_or(ClientError::NoAddress)?;
        let stream =
            TcpStream::connect_timeout(&resolved, timeout).map_err(ClientError::Connect)?;
        // Commands are tiny and latency-sensitive; Nagle would coalesce a retune with
        // whatever is sent next.
        stream.set_nodelay(true).map_err(ClientError::Connect)?;
        Self::from_stream(stream)
    }

    /// Read the greeting from an already-connected stream.
    ///
    /// # Errors
    ///
    /// [`ClientError::Handshake`] when the greeting is incomplete and
    /// [`ClientError::NotRtlTcp`] when its magic is not `RTL0`.
    pub fn from_stream(mut stream: TcpStream) -> Result<Self, ClientError> {
        let mut wire = [0u8; Greeting::LEN];
        stream
            .read_exact(&mut wire)
            .map_err(ClientError::Handshake)?;
        let greeting = Greeting::decode_exact(&wire).map_err(|e| match e.kind {
            bnb::ErrorKind::BadMagic { .. } => {
                let [a, b, c, d, ..] = wire;
                ClientError::NotRtlTcp {
                    magic: [a, b, c, d],
                }
            }
            _ => ClientError::Codec(e),
        })?;
        Ok(Self { stream, greeting })
    }

    /// The greeting the server sent.
    #[must_use]
    pub const fn greeting(&self) -> Greeting {
        self.greeting
    }

    /// The underlying socket.
    #[must_use]
    pub const fn get_ref(&self) -> &TcpStream {
        &self.stream
    }

    /// Send any command.
    ///
    /// # Errors
    ///
    /// [`ClientError::Command`] when the write fails.
    pub fn send(&mut self, command: Command) -> Result<(), ClientError> {
        let wire = command.to_bytes()?;
        self.stream
            .write_all(&wire)
            .map_err(|source| ClientError::Command {
                opcode: command.opcode,
                source,
            })
    }

    /// Tune to `hz`.
    ///
    /// # Errors
    ///
    /// See [`Client::send`].
    pub fn set_center_frequency(&mut self, hz: u32) -> Result<(), ClientError> {
        self.send(Command::set_center_frequency(hz))
    }

    /// Stream at `samples_per_second`.
    ///
    /// # Errors
    ///
    /// See [`Client::send`].
    pub fn set_sample_rate(&mut self, samples_per_second: u32) -> Result<(), ClientError> {
        self.send(Command::set_sample_rate(samples_per_second))
    }

    /// Manual (`true`) or automatic (`false`) tuner gain.
    ///
    /// # Errors
    ///
    /// See [`Client::send`].
    pub fn set_gain_mode(&mut self, manual: bool) -> Result<(), ClientError> {
        self.send(Command::set_gain_mode(manual))
    }

    /// Manual tuner gain in tenths of a dB.
    ///
    /// # Errors
    ///
    /// See [`Client::send`].
    pub fn set_gain(&mut self, tenths_db: u32) -> Result<(), ClientError> {
        self.send(Command::set_gain(tenths_db))
    }

    /// Automatic gain correction on or off.
    ///
    /// # Errors
    ///
    /// See [`Client::send`].
    pub fn set_agc_mode(&mut self, on: bool) -> Result<(), ClientError> {
        self.send(Command::set_agc_mode(on))
    }

    /// Tuner gain by gain-table index.
    ///
    /// # Errors
    ///
    /// See [`Client::send`].
    pub fn set_tuner_gain_by_index(&mut self, index: u32) -> Result<(), ClientError> {
        self.send(Command::set_tuner_gain_by_index(index))
    }

    /// Fill `buf` from the sample stream.
    ///
    /// # Errors
    ///
    /// [`ClientError::Closed`] when the stream ends first, [`ClientError::Read`] otherwise.
    pub fn read_exact(&mut self, buf: &mut [u8]) -> Result<(), ClientError> {
        self.stream.read_exact(buf).map_err(|e| match e.kind() {
            ErrorKind::UnexpectedEof => ClientError::Closed,
            _ => ClientError::Read(e),
        })
    }

    /// Discard whatever stream bytes have already arrived, without waiting for more.
    ///
    /// The socket is returned to blocking mode whether or not the drain succeeds.
    ///
    /// # Errors
    ///
    /// [`ClientError::Closed`] when the server has closed the connection,
    /// [`ClientError::Drain`] otherwise.
    pub fn drain_buffered(&mut self) -> Result<(), ClientError> {
        self.stream
            .set_nonblocking(true)
            .map_err(ClientError::Drain)?;

        let mut buf = [0u8; DISCARD_CHUNK];
        let drained = loop {
            match self.stream.read(&mut buf) {
                Ok(0) => break Err(ClientError::Closed),
                Err(e) if e.kind() == ErrorKind::WouldBlock => break Ok(()),
                Ok(_) => {}
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) => break Err(ClientError::Drain(e)),
            }
        };

        // Leaving the socket non-blocking would turn every later read into WouldBlock.
        self.stream
            .set_nonblocking(false)
            .map_err(ClientError::Drain)?;
        drained
    }

    /// Read and discard exactly `bytes` stream bytes, waiting for them to arrive.
    ///
    /// # Errors
    ///
    /// [`ClientError::Closed`] when the stream ends first, [`ClientError::Drain`] otherwise.
    pub fn discard(&mut self, bytes: usize) -> Result<(), ClientError> {
        let mut buf = [0u8; DISCARD_CHUNK];
        let mut remaining = bytes;
        while remaining > 0 {
            let take = remaining.min(DISCARD_CHUNK);
            self.stream
                .read_exact(&mut buf[..take])
                .map_err(|e| match e.kind() {
                    ErrorKind::UnexpectedEof => ClientError::Closed,
                    _ => ClientError::Drain(e),
                })?;
            remaining -= take;
        }
        Ok(())
    }
}
