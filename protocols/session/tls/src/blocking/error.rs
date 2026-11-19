//! Why a blocking stream failed.

// --- Standard library ---
use std::io;

// --- Workspace dependencies ---
use thiserror::Error as ThisError;

// --- Internal modules ---
use crate::client;

/// Why a [`Stream`](super::Stream) operation failed: the transport, or the TLS connection
/// on it.
#[derive(Debug, ThisError)]
pub enum Error {
    /// The transport failed.
    #[error("transport failed")]
    Io(#[from] io::Error),
    /// The TLS connection failed or refused the call.
    #[error("TLS connection failed")]
    Tls(#[from] client::Error),
}

impl From<Error> for io::Error {
    /// The form `Read` and `Write` report: a transport error unchanged; a connection cut
    /// short as `UnexpectedEof`; a call the connection no longer allows as `NotConnected`;
    /// any other TLS failure as `InvalidData`. The TLS error stays reachable as the source.
    fn from(error: Error) -> Self {
        let kind = match error {
            Error::Io(error) => return error,
            Error::Tls(client::Error::Truncated) => io::ErrorKind::UnexpectedEof,
            Error::Tls(
                client::Error::InvalidState
                | client::Error::Record(crate::record::RecordError::KeyUsageLimit),
            ) => io::ErrorKind::NotConnected,
            Error::Tls(_) => io::ErrorKind::InvalidData,
        };
        Self::new(kind, error)
    }
}
