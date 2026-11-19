//! Why a connection failed.

// --- Workspace dependencies ---
use bnb::BitError;
use crypto::CryptoError;
use thiserror::Error;

// --- Internal modules ---
use crate::{
    CertificateError,
    handshake::MessageTooLong,
    record::RecordError,
    wire::{Alert, AlertDescription, ExtensionType},
};

/// Why a [`Client`](super::Client) call failed. Every error ends the connection except
/// [`Error::InvalidState`] and a write refused with [`RecordError::KeyUsageLimit`] during closure.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum Error {
    /// A record could not be framed, opened or sealed.
    #[error("record layer failure")]
    Record(#[from] RecordError),
    /// A message could not be decoded from the peer's bytes, or built for sending.
    #[error("malformed {what}")]
    Codec {
        /// What was being decoded or encoded.
        what: &'static str,
        /// The codec's reason.
        #[source]
        source: BitError,
    },
    /// A handshake message announced too long a body.
    #[error(transparent)]
    MessageTooLong(#[from] MessageTooLong),
    /// The server's certificate chain was refused.
    #[error("server certificate refused")]
    Certificate(#[from] CertificateError),
    /// A cryptographic operation failed for a reason that is not the peer's doing, such as
    /// the random source running dry.
    #[error("cryptographic failure")]
    Crypto(#[from] CryptoError),
    /// The peer sent something the current state does not allow.
    #[error("unexpected message: {0}")]
    UnexpectedMessage(&'static str),
    /// A field of a well-formed message has a value the handshake cannot accept.
    #[error("illegal parameter: {0}")]
    IllegalParameter(&'static str),
    /// The server chose a protocol version other than TLS 1.2.
    #[error("server did not choose TLS 1.2")]
    ProtocolVersion,
    /// The server sent an extension that was unoffered, repeated, or invalid in a `ServerHello`.
    #[error("unsupported server hello extension {0:?}")]
    UnsupportedExtension(ExtensionType),
    /// The two ends cannot agree on parameters this client accepts.
    #[error("handshake failure: {0}")]
    HandshakeFailure(&'static str),
    /// The server's key-exchange signature or its `Finished` check value did not verify.
    #[error("{0} did not verify")]
    DecryptError(&'static str),
    /// The peer ended the connection with this alert.
    #[error("peer sent alert {0:?}")]
    PeerAlert(Alert),
    /// The transport ended before the peer said it was closing, so data may be missing.
    #[error("connection ended without close_notify")]
    Truncated,
    /// The call is not allowed now: the connection is over, this end has closed, or the
    /// handshake has not finished. The connection's state is unchanged.
    #[error("not allowed in the connection's current state")]
    InvalidState,
}

impl Error {
    /// The fatal alert that tells the peer about this error, or `None` when the peer should
    /// be told nothing (it caused the end itself, or the error is the caller's).
    #[must_use]
    pub const fn alert(&self) -> Option<AlertDescription> {
        Some(match self {
            Self::Record(RecordError::RecordOverflow) => AlertDescription::RecordOverflow,
            // RFC 5288 §3: every failure to open a record is reported the same way.
            Self::Record(RecordError::BadRecordMac | RecordError::Truncated) => {
                AlertDescription::BadRecordMac
            }
            Self::Record(RecordError::KeyUsageLimit)
            | Self::PeerAlert(_)
            | Self::Truncated
            | Self::InvalidState => return None,
            // What is left of `Record` can only come from sealing one of our own records.
            Self::Record(_) | Self::Crypto(_) => AlertDescription::InternalError,
            Self::Codec { .. } | Self::MessageTooLong(_) => AlertDescription::DecodeError,
            Self::Certificate(CertificateError::Path(_)) => AlertDescription::CertificateUnknown,
            Self::Certificate(_) => AlertDescription::BadCertificate,
            Self::UnexpectedMessage(_) => AlertDescription::UnexpectedMessage,
            Self::IllegalParameter(_) => AlertDescription::IllegalParameter,
            Self::ProtocolVersion => AlertDescription::ProtocolVersion,
            Self::UnsupportedExtension(_) => AlertDescription::UnsupportedExtension,
            Self::HandshakeFailure(_) => AlertDescription::HandshakeFailure,
            Self::DecryptError(_) => AlertDescription::DecryptError,
        })
    }
}
