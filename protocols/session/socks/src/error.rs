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
    /// A codec, finite-EOF truncation, or receive-capacity check failed.
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
    /// Guided CONNECT requires a nonzero destination port; raw endpoints do not.
    /// Servers report this as a general failure, not a destination dial failure.
    #[error("CONNECT destination port must be nonzero")]
    ZeroDestinationPort,
    /// No resolved target was permitted by the caller's destination policy.
    #[error("destination denied by policy")]
    PermissionDenied,
    /// Timeouts must be nonzero and concurrency must be at least one.
    #[error("invalid proxy limits")]
    InvalidLimits,
    /// A proxy worker thread or task panicked.
    #[error("proxy worker panicked")]
    WorkerPanicked,
    /// A resumable session was used after failure/handoff or in the wrong phase.
    #[error("operation is not valid in the current SOCKS session phase")]
    InvalidState,
    /// Server construction requires at least one explicitly accepted protocol.
    #[error("no accepted SOCKS protocols configured")]
    NoProtocols,
    /// Guided construction requires an explicit security policy.
    #[error("no SOCKS server policy configured")]
    MissingPolicy,
    /// Client construction requires an explicit version-specific configuration.
    #[error("no SOCKS client protocol configured")]
    MissingProtocol,
    /// The peer's version is not accepted by this server.
    #[error("SOCKS version {0} is not accepted")]
    VersionNotAccepted(u8),
}

impl From<bnb::net::MessageReadError> for Error {
    fn from(error: bnb::net::MessageReadError) -> Self {
        match error {
            bnb::net::MessageReadError::Io(error) => Self::Io(error),
            bnb::net::MessageReadError::Codec(error) => Self::Codec(error),
            // Preserve a future reader error's source without guessing its meaning.
            other => Self::Io(std::io::Error::other(other)),
        }
    }
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

#[cfg(test)]
mod unit {
    use super::Error;
    use bnb::net::MessageReadError;
    use bnb::{BitError, ErrorKind};
    use std::io;

    #[test]
    fn reader_codec_error_preserves_kind_position_and_field() {
        let original = BitError::new(
            ErrorKind::UnexpectedEof {
                needed: 16,
                remaining: 8,
            },
            24,
        )
        .in_field("port");
        let converted = Error::from(MessageReadError::Codec(original.clone()));
        let Error::Codec(error) = converted else {
            panic!("reader codec errors must retain their classification");
        };
        assert_eq!(error, original);
    }

    #[test]
    fn reader_io_error_preserves_owned_source() {
        #[derive(Debug, thiserror::Error)]
        #[error("scripted transport failure")]
        struct TransportFault(u32);

        let original = io::Error::new(io::ErrorKind::ConnectionReset, TransportFault(42));
        let converted = Error::from(MessageReadError::Io(original));
        let error = std::error::Error::source(&converted)
            .unwrap()
            .downcast_ref::<io::Error>()
            .expect("the original I/O error remains the direct source");
        assert_eq!(error.kind(), io::ErrorKind::ConnectionReset);
        let fault = error
            .get_ref()
            .unwrap()
            .downcast_ref::<TransportFault>()
            .expect("the owned transport source must not be stringified");
        assert_eq!(fault.0, 42);
    }

    #[test]
    fn reader_io_error_preserves_raw_os_code() {
        let original = io::Error::from_raw_os_error(13);
        let kind = original.kind();
        let Error::Io(error) = Error::from(MessageReadError::Io(original)) else {
            panic!("reader transport errors must retain their classification");
        };
        assert_eq!(error.kind(), kind);
        assert_eq!(error.raw_os_error(), Some(13));
    }
}
