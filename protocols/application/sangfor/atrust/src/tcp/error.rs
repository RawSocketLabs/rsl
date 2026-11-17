//! Why a tunnel could not be opened or failed.

// --- Standard library ---
use std::io;

// --- Workspace dependencies ---
use sangfor_atrust_wire::tcp::ReplyCode;
use thiserror::Error as ThisError;

/// A tunnel that could not be opened, or that failed.
#[derive(Debug, ThisError)]
#[non_exhaustive]
pub enum Error {
    /// The connection to the node failed.
    #[error("the connection to the node failed")]
    Io(#[from] io::Error),
    /// A protocol primitive refused its input.
    #[error("a protocol primitive failed")]
    Sig(#[from] sangfor_atrust_sig::Error),
    /// The request cannot be put on the wire: the signed document or the destination name is
    /// longer than its length field can say.
    #[error("the tunnel request is too long to encode")]
    TooLong,
    /// The node answered with something the protocol does not allow at that point.
    #[error("the node broke the protocol: {0}")]
    Protocol(&'static str),
    /// The node no longer knows the session: a full login is needed.
    #[error("the session is not valid (code {code})")]
    SessionInvalid {
        /// The node's code.
        code: i64,
    },
    /// The node refused to authorise the stream.
    #[error("the node refused the stream: {code}: {message}")]
    Refused {
        /// The node's code.
        code: i64,
        /// The node's message, which may be empty.
        message: String,
    },
    /// The node could not connect to the destination.
    #[error("the node could not connect: {0:?}")]
    Connect(ReplyCode),
}
