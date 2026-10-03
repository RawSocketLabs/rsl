//! Raw TFTP transport for testing and research.
//!
//! [`RawClient`] is the deliberately *un-orchestrated* counterpart to
//! [`Client`](super::Client): it runs no transfer state machine, no
//! retransmission, no TID checks, and no option negotiation. It sends exactly
//! the packets (or bytes) it is given and hands back whatever arrives, so a
//! caller can probe how a TFTP implementation responds to malformed or hostile
//! traffic (fuzzing, interop testing, security research).
//!
//! The compliant path lives in [`Client`](super::Client); the [`malformed`]
//! module provides ready-made packets that intentionally violate RFC 1350.

use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

use crate::error::Result;
use crate::transfer::RECV_BUFFER;
use crate::wire::Packet;

/// A validation-free TFTP transport over a single UDP socket.
pub struct RawClient {
    socket: UdpSocket,
    /// The default destination for [`send`](RawClient::send) /
    /// [`send_bytes`](RawClient::send_bytes); public so it can be retargeted.
    pub server: SocketAddr,
}

impl RawClient {
    /// Binds a fresh local UDP socket and remembers `server` as the default
    /// destination. Sends nothing.
    ///
    /// # Errors
    /// Returns an error if the socket cannot be bound.
    pub fn bind(server: SocketAddr) -> Result<Self> {
        Ok(Self {
            socket: UdpSocket::bind((std::net::Ipv4Addr::UNSPECIFIED, 0))?,
            server,
        })
    }

    /// Wraps an already-bound socket.
    pub fn from_socket(socket: UdpSocket, server: SocketAddr) -> Self {
        Self { socket, server }
    }

    /// The underlying socket, for direct manipulation.
    pub fn socket(&self) -> &UdpSocket {
        &self.socket
    }

    /// Sets a read timeout so a silent peer cannot block reads forever.
    ///
    /// # Errors
    /// Returns an error if the timeout cannot be applied.
    pub fn set_read_timeout(&self, timeout: Option<Duration>) -> Result<()> {
        Ok(self.socket.set_read_timeout(timeout)?)
    }

    /// Encodes and sends a packet to the default `server`.
    ///
    /// # Errors
    /// Returns an error if the send fails.
    pub fn send(&self, packet: &Packet) -> Result<()> {
        self.send_to(&packet.encode(), self.server)
    }

    /// Sends raw bytes verbatim to the default `server` — no encoding, no
    /// validation. Use for truncated packets, oversized fields, or garbage.
    ///
    /// # Errors
    /// Returns an error if the send fails.
    pub fn send_bytes(&self, bytes: &[u8]) -> Result<()> {
        self.send_to(bytes, self.server)
    }

    /// Sends raw bytes to an explicit destination.
    ///
    /// # Errors
    /// Returns an error if the send fails.
    pub fn send_to(&self, bytes: &[u8], dest: SocketAddr) -> Result<()> {
        self.socket.send_to(bytes, dest)?;
        Ok(())
    }

    /// Receives the next datagram and decodes it into a [`Packet`], returning it
    /// with the source address.
    ///
    /// # Errors
    /// Returns an error if the receive fails or the datagram cannot be decoded.
    pub fn recv(&self) -> Result<(Packet, SocketAddr)> {
        let (bytes, src) = self.recv_bytes()?;
        Ok((Packet::decode(&bytes)?, src))
    }

    /// Receives the next datagram as raw bytes plus the source address.
    ///
    /// # Errors
    /// Returns an error if the receive fails.
    pub fn recv_bytes(&self) -> Result<(Vec<u8>, SocketAddr)> {
        let mut buf = vec![0u8; RECV_BUFFER];
        let (n, src) = self.socket.recv_from(&mut buf)?;
        buf.truncate(n);
        Ok((buf, src))
    }
}

/// Ready-made packets and byte strings that intentionally violate RFC 1350, for
/// exercising a server's handling of malformed input.
pub mod malformed {
    use crate::wire::{Data, ErrorCode, ErrorPacket};

    /// A Read Request whose filename is **not** NUL-terminated — the datagram
    /// ends mid-string, so a strict decoder cannot frame it.
    pub fn unterminated_request() -> Vec<u8> {
        let mut bytes = vec![0x00, 0x01]; // RRQ opcode
        bytes.extend_from_slice(b"truncated-filename"); // no trailing NUL
        bytes
    }

    /// A datagram with an opcode (99) that has no defined packet type.
    pub fn unknown_opcode() -> Vec<u8> {
        vec![0x00, 0x63, 0x00, 0x00]
    }

    /// A DATA block whose payload exceeds the 512-byte default — representable
    /// on the wire, but non-conformant absent a negotiated `blksize`.
    pub fn oversized_data_block() -> Data {
        Data::new(1, vec![0xAB; 1024])
    }

    /// An ERROR packet carrying an unassigned error code, to check that a peer
    /// tolerates `Custom` codes.
    pub fn bogus_error_code() -> ErrorPacket {
        ErrorPacket::new(ErrorCode::Custom(0x4242), "synthetic")
    }
}
