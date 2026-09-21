//! Errors from the guided CONNECT session and proxy layers, not wire-codec policy.

use crate::v5::{AuthMethod, Command, Endpoint, ReplyCode};
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
    Codec(#[source] bnb::BitError),
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

/// Translate only typed endpoint dispatch misses at the guided protocol boundary.
impl From<bnb::BitError> for Error {
    fn from(error: bnb::BitError) -> Self {
        let code = error
            .dispatch_error_for::<Endpoint>()
            .and_then(bnb::EnumDispatchError::observed)
            .and_then(|value| match value {
                bnb::DispatchValue::Integer(code) => u8::try_from(*code).ok(),
                _ => None,
            });
        match code {
            Some(code) => Self::UnsupportedAddressType(code),
            None => Self::Codec(error),
        }
    }
}

impl From<bnb::net::MessageReadError> for Error {
    fn from(error: bnb::net::MessageReadError) -> Self {
        match error {
            bnb::net::MessageReadError::Io(error) => Self::Io(error),
            bnb::net::MessageReadError::Codec(error) => Self::from(error),
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
    fn endpoint_dispatch_misses_convert_directly_and_through_readers() {
        for code in (0..=u8::MAX).filter(|code| ![1, 3, 4].contains(code)) {
            let original = crate::v5::Request::decode_exact(&[5, 1, 0, code]).unwrap_err();
            for converted in [
                Error::from(original.clone()),
                Error::from(MessageReadError::Codec(original)),
            ] {
                assert!(
                    matches!(converted, Error::UnsupportedAddressType(actual) if actual == code)
                );
            }
        }
    }

    #[test]
    fn unrelated_dispatch_errors_retain_all_details() {
        // Deliberately shares the diagnostic name; only type identity may match.
        #[bnb::bin(big)]
        #[derive(Debug)]
        enum Endpoint {
            #[bin(magic = 1u8)]
            Known,
        }
        let original = Endpoint::decode_exact(&[255]).unwrap_err();
        for converted in [
            Error::from(original.clone()),
            Error::from(MessageReadError::Codec(original.clone())),
        ] {
            let Error::Codec(error) = converted else {
                panic!("unrelated enum must stay a codec error");
            };
            assert_eq!(error, original);
        }
    }

    #[test]
    fn uncaptured_noninteger_and_overwide_codes_remain_codec_errors() {
        // Defensive conversion for hand-written codecs or future Endpoint shapes. These
        // states are reachable only through bnb's doc-hidden constructor, which no API gate
        // tracks; lockstep workspace development surfaces a signature change at compile time.
        for observed in [
            None,
            Some(bnb::DispatchValue::Bytes(vec![255])),
            Some(bnb::DispatchValue::Integer(256)),
        ] {
            let original = BitError::no_matching_variant(
                std::any::TypeId::of::<crate::v5::Endpoint>(),
                &("Endpoint", bnb::DispatchKind::Magic),
                observed,
                32,
            )
            .in_field("magic");
            let Error::Codec(error) = Error::from(original.clone()) else {
                panic!("only captured u8 integers are address types");
            };
            assert_eq!(error, original);
        }
    }

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
