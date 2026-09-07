//! Errors from the guided CONNECT session and proxy layers, not wire-codec policy.

use crate::v5::{AuthMethod, Command, ReplyCode};
use thiserror::Error;

/// A failed session is terminal: drop its transport; do not retry the handshake on it.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// The transport, resolver, or target connection failed.
    #[error("SOCKS I/O failed: {0}")]
    Io(#[from] std::io::Error),
    /// A complete frame could not be decoded or encoded.
    #[error("SOCKS codec failed: {0}")]
    Codec(#[from] bnb::BitError),
    /// Locally supplied credentials or a message failed construction validation.
    #[error("invalid SOCKS construction: {0}")]
    Construction(#[from] bnb::BuilderError),
    /// The peer used the wrong protocol or subnegotiation version.
    #[error("expected version {expected}, received {actual}")]
    Version {
        /// Required version for this phase.
        expected: u8,
        /// Peer-supplied version.
        actual: u8,
    },
    /// A session header carried a nonzero reserved byte.
    #[error("nonzero SOCKS reserved byte: {0}")]
    Reserved(u8),
    /// The peer and local policy have no acceptable method in common.
    #[error("no acceptable authentication method")]
    NoAcceptableMethod,
    /// The server selected a method the client did not offer.
    #[error("server selected an unoffered method: {0:?}")]
    UnexpectedMethod(AuthMethod),
    /// Credentials were rejected; no credentials are retained in the error.
    #[error("username/password authentication rejected")]
    AuthenticationRejected,
    /// The proxy returned a failing reply, including an unknown wire code.
    #[error("SOCKS request failed: {0:?}")]
    Reply(ReplyCode),
    /// Only CONNECT is supported by this session layer.
    #[error("unsupported SOCKS command: {0:?}")]
    UnsupportedCommand(Command),
    /// The address payload cannot be framed without knowing its type.
    #[error("unsupported SOCKS address type: {0:#04x}")]
    UnsupportedAddressType(u8),
    /// The guided path requires a nonempty, encodable domain; system DNS needs UTF-8.
    #[error("invalid destination domain")]
    InvalidEndpoint,
    /// No resolved target was permitted by the caller's destination policy.
    #[error("destination denied by policy")]
    PermissionDenied,
    /// Timeouts must be nonzero and concurrency must be at least one.
    #[error("invalid proxy limits")]
    InvalidLimits,
    /// A proxy worker thread or task panicked.
    #[error("proxy worker panicked")]
    WorkerPanicked,
}

impl Error {
    pub(crate) fn reply_code(&self) -> ReplyCode {
        match self {
            Self::UnsupportedCommand(_) => ReplyCode::CommandNotSupported,
            Self::UnsupportedAddressType(_) => ReplyCode::AddressTypeNotSupported,
            Self::PermissionDenied => ReplyCode::ConnectionNotAllowed,
            Self::InvalidEndpoint => ReplyCode::HostUnreachable,
            Self::Io(error) => match error.kind() {
                std::io::ErrorKind::ConnectionRefused => ReplyCode::ConnectionRefused,
                std::io::ErrorKind::PermissionDenied => ReplyCode::ConnectionNotAllowed,
                std::io::ErrorKind::NotFound | std::io::ErrorKind::AddrNotAvailable => {
                    ReplyCode::HostUnreachable
                }
                _ => ReplyCode::GeneralFailure,
            },
            _ => ReplyCode::GeneralFailure,
        }
    }
}
