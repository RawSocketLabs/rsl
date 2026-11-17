//! Why a blocking exchange failed.

// --- Standard library ---
use std::io;

// --- Workspace dependencies ---
use thiserror::Error as ThisError;

/// Why an [`exchange`](super::exchange) failed: the transport, or what the server sent.
#[derive(Debug, ThisError)]
pub enum Error {
    /// The transport failed.
    #[error("transport failed")]
    Io(#[from] io::Error),
    /// The response could not be decoded.
    #[error("unusable response")]
    Http(#[from] crate::Error),
}
