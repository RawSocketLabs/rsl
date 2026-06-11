//! The crate's error type and result alias.
//!
//! Every fallible operation returns [`Result<T>`], which is
//! [`std::result::Result`] specialized to [`TftpError`]. The variants separate
//! transport failures ([`Io`](TftpError::Io)), malformed wire data
//! ([`Decode`](TftpError::Decode)), a peer-reported protocol error carrying the
//! wire [`ErrorCode`] ([`Peer`](TftpError::Peer)), state-machine surprises
//! ([`Unexpected`](TftpError::Unexpected)), retransmission exhaustion
//! ([`TimedOut`](TftpError::TimedOut)), and local validation
//! ([`Validation`](TftpError::Validation)).

use std::io;
use std::net::AddrParseError;

use thiserror::Error;

use crate::wire::{ErrorCode, ErrorPacket};

/// Every error this crate can produce.
#[derive(Error, Debug)]
pub enum TftpError {
    /// An I/O error from the underlying socket.
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// A socket address failed to parse (e.g. from a string literal).
    #[error("failed to parse address: {0}")]
    AddressParse(#[from] AddrParseError),

    /// A datagram could not be decoded into a typed packet.
    #[error("malformed TFTP packet: {0}")]
    Decode(String),

    /// The peer sent an ERROR packet; the transfer is over.
    #[error("peer reported TFTP error {code:?}: {message}")]
    Peer {
        /// The wire error code.
        code: ErrorCode,
        /// The human-readable message (lossily decoded).
        message: String,
    },

    /// A well-formed packet arrived that the state machine did not expect.
    #[error("unexpected packet: {0}")]
    Unexpected(String),

    /// A packet went unacknowledged through all retransmission attempts.
    #[error("transfer timed out after {retries} retransmission(s)")]
    TimedOut {
        /// How many retransmissions were attempted before giving up.
        retries: u32,
    },

    /// A local precondition failed (e.g. an empty filename, an unusable option).
    #[error("validation error: {0}")]
    Validation(String),
}

impl TftpError {
    /// Builds a [`Peer`](TftpError::Peer) error from a received ERROR packet.
    pub fn from_error_packet(packet: &ErrorPacket) -> Self {
        TftpError::Peer {
            code: packet.code,
            message: packet.message_lossy(),
        }
    }
}

/// A `Result` specialized to [`TftpError`].
pub type Result<T> = std::result::Result<T, TftpError>;

impl From<binrw::Error> for TftpError {
    fn from(err: binrw::Error) -> Self {
        match err {
            binrw::Error::Io(e) => TftpError::Io(e),
            other => TftpError::Decode(other.to_string()),
        }
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn io_conversion() {
        let err = TftpError::from(io::Error::other("boom"));
        assert!(matches!(err, TftpError::Io(_)));
    }

    #[test]
    fn peer_from_error_packet() {
        let packet = ErrorPacket::new(ErrorCode::FileNotFound, "missing");
        let err = TftpError::from_error_packet(&packet);
        assert!(matches!(
            err,
            TftpError::Peer { code: ErrorCode::FileNotFound, .. }
        ));
    }
}
