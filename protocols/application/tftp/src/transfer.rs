//! Transfer primitives shared by the client and server state machines.
//!
//! TFTP is lock-step: each side sends one packet and waits for the matching
//! reply, retransmitting on a timeout. The transfer identifier (TID) is the
//! UDP port pair agreed in the first exchange; datagrams from any other source
//! are answered with an ERROR(5) and otherwise ignored (RFC 1350 §4). These
//! helpers capture that mechanism so the client and server share one
//! implementation.

use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

use crate::error::{Result, TftpError};
use crate::wire::{Ack, Data, ErrorCode, ErrorPacket, Packet};

/// The default DATA block size, in octets (RFC 1350 §5). A block shorter than
/// this ends the transfer.
pub const BLOCK_SIZE: usize = 512;

/// The largest block size negotiable via the `blksize` option (RFC 2348).
pub const MAX_BLOCK_SIZE: usize = 65464;

/// The default per-packet retransmission timeout.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(3);

/// The default number of retransmissions before a transfer is abandoned.
pub const DEFAULT_MAX_RETRIES: u32 = 5;

/// A receive buffer sized for the largest possible packet (max block + the
/// 4-byte opcode/block header).
pub(crate) const RECV_BUFFER: usize = MAX_BLOCK_SIZE + 4;

/// Sends an ERROR packet to `dest`, best-effort (an ERROR is a courtesy and is
/// never retransmitted, per RFC 1350 §2/§7).
//~ implements rfc1350#4/should.b274ab part="error to the source of a stray packet"
//~ implements rfc1350#2/may.4a695c part="ERROR is sent unacknowledged, not retransmitted"
//~ implements rfc1350#7/may.fe883f part="ERROR is a best-effort courtesy"
pub(crate) fn send_error(socket: &UdpSocket, dest: SocketAddr, code: ErrorCode, message: &str) {
    let packet = ErrorPacket::new(code, message).encode();
    let _ = socket.send_to(&packet, dest);
}

/// Whether an I/O error is a read-timeout (the platform-specific kind that
/// [`UdpSocket::set_read_timeout`] produces).
fn is_timeout(err: &io::Error) -> bool {
    matches!(
        err.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    )
}

/// Receives the next decodable packet, applying the lock-step machinery:
///
/// * On a read timeout, retransmits `last_sent` to `retransmit_to` and counts a
///   retry; after `max_retries` timeouts it returns [`TftpError::TimedOut`].
/// * A datagram from a source other than `expected` (once a TID is locked) is
///   answered with ERROR(5) and ignored without disturbing the transfer.
/// * An undecodable datagram is ignored; the wait continues.
///
/// `expected` is `None` for the very first exchange, where the peer's TID is not
/// yet known and any source is accepted (and should be locked by the caller).
//~ implements rfc1350#4/should.c9847d part="source TID must match"
//~ implements rfc1350#4/should.06d78b part="discard packets from a wrong TID"
//~ implements rfc1350#4/should.7ad217 part="a second/stray response is rejected, not adopted"
//~ implements rfc1350#7/must.b39dff part="timeouts detect errors / drive retransmission"
//~ implements rfc1350#2/may.f7dde1 part="recipient retransmits its last packet on timeout"
pub(crate) fn recv_packet(
    socket: &UdpSocket,
    buf: &mut [u8],
    expected: Option<SocketAddr>,
    last_sent: &[u8],
    retransmit_to: SocketAddr,
    max_retries: u32,
) -> Result<(Packet, SocketAddr)> {
    let mut retries = 0;
    loop {
        match socket.recv_from(buf) {
            Ok((n, src)) => {
                if let Some(tid) = expected {
                    if src != tid {
                        tracing::debug!(%src, expected = %tid, "ignoring packet from wrong TID");
                        send_error(
                            socket,
                            src,
                            ErrorCode::UnknownTransferId,
                            "unknown transfer ID",
                        );
                        continue;
                    }
                }
                match Packet::decode(&buf[..n]) {
                    Ok(packet) => return Ok((packet, src)),
                    Err(err) => {
                        tracing::debug!(%src, %err, "ignoring undecodable datagram");
                        continue;
                    }
                }
            }
            Err(err) if is_timeout(&err) => {
                if retries >= max_retries {
                    return Err(TftpError::TimedOut { retries });
                }
                retries += 1;
                tracing::debug!(retries, "retransmitting after timeout");
                socket.send_to(last_sent, retransmit_to)?;
            }
            Err(err) => return Err(err.into()),
        }
    }
}

/// Sends `data` to `peer` as a run of DATA blocks, starting at block 1, each
/// retransmitted until acknowledged. Returns once a block shorter than `blksize`
/// (possibly an empty one, when the length is an exact multiple) has been ACKed.
///
/// This is the sender half of a transfer — used by the client's upload and the
/// server's download.
//~ implements rfc1350#6/must.c9764b part="retransmit last DATA until acknowledged"
//~ implements rfc1350#2/must.915d43 part="each DATA awaits its ACK before the next is sent"
//~ implements rfc1350#6/may.fd886c part="terminate after the final ACK"
pub(crate) fn send_file(
    socket: &UdpSocket,
    peer: SocketAddr,
    data: &[u8],
    blksize: usize,
    max_retries: u32,
) -> Result<()> {
    let mut buf = vec![0u8; RECV_BUFFER];
    let mut block: u16 = 1;
    let mut offset = 0usize;
    loop {
        let end = (offset + blksize).min(data.len());
        let chunk = &data[offset..end];
        let packet = Data::new(block, chunk.to_vec()).encode();
        socket.send_to(&packet, peer)?;
        await_ack(socket, &mut buf, peer, block, &packet, max_retries)?;

        let is_final = chunk.len() < blksize;
        offset = end;
        block = block.wrapping_add(1);
        if is_final {
            return Ok(());
        }
    }
}

/// Waits for the ACK of `block`, retransmitting `data_packet` on timeout. A
/// stale ACK for an earlier block is ignored — the Sorcerer's Apprentice guard.
fn await_ack(
    socket: &UdpSocket,
    buf: &mut [u8],
    peer: SocketAddr,
    block: u16,
    data_packet: &[u8],
    max_retries: u32,
) -> Result<()> {
    loop {
        let (packet, _) = recv_packet(socket, buf, Some(peer), data_packet, peer, max_retries)?;
        match packet {
            Packet::Ack(a) if a.block == block => return Ok(()),
            Packet::Ack(_) => continue,
            Packet::Error(err) => return Err(TftpError::from_error_packet(&err)),
            other => {
                return Err(TftpError::Unexpected(format!(
                    "expected ACK({block}), got {:?}",
                    other.opcode()
                )));
            }
        }
    }
}

/// Receives DATA blocks into `out`, starting at `start_block`, ACKing each and
/// retransmitting `last_sent` (the ACK or OACK-ACK that primes the first block)
/// on timeout. Returns once a block shorter than `blksize` ends the transfer.
///
/// This is the receiver half of a transfer — used by the client's download and
/// the server's upload. A duplicate of the previously-ACKed block is re-ACKed
/// without re-appending.
//~ implements rfc1350#2/must.915d43 part="receiver ACKs each block before the next arrives"
pub(crate) fn recv_file(
    socket: &UdpSocket,
    peer: SocketAddr,
    start_block: u16,
    blksize: usize,
    max_retries: u32,
    mut last_sent: Vec<u8>,
    out: &mut Vec<u8>,
) -> Result<()> {
    let mut buf = vec![0u8; RECV_BUFFER];
    let mut expected = start_block;
    loop {
        let (packet, _) = recv_packet(socket, &mut buf, Some(peer), &last_sent, peer, max_retries)?;
        match packet {
            Packet::Data(d) if d.block == expected => {
                out.extend_from_slice(&d.payload);
                last_sent = Ack::new(expected).encode();
                socket.send_to(&last_sent, peer)?;
                let is_final = d.payload.len() < blksize;
                expected = expected.wrapping_add(1);
                if is_final {
                    return Ok(());
                }
            }
            // Duplicate of the block we just acknowledged: re-ACK, don't append.
            Packet::Data(d) if d.block == expected.wrapping_sub(1) => {
                socket.send_to(&Ack::new(d.block).encode(), peer)?;
            }
            Packet::Data(_) => { /* out-of-window block: ignore */ }
            Packet::Error(err) => return Err(TftpError::from_error_packet(&err)),
            other => {
                return Err(TftpError::Unexpected(format!(
                    "expected DATA, got {:?}",
                    other.opcode()
                )));
            }
        }
    }
}
