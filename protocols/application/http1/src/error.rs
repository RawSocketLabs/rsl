//! Why a message could not be built or decoded.

// --- Workspace dependencies ---
use thiserror::Error as ThisError;

/// Why a request could not be built or a response could not be decoded. A decoding error is
/// final for the connection: its framing can no longer be trusted.
#[derive(Clone, Debug, ThisError, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// A guided request constructor was given something that cannot go in a request without
    /// changing its structure.
    #[error("invalid request: {0}")]
    InvalidRequest(&'static str),
    /// The response head is not valid HTTP/1 syntax.
    #[error("malformed response head: {0}")]
    Malformed(&'static str),
    /// The response's body length cannot be determined safely.
    #[error("unusable response framing: {0}")]
    Framing(&'static str),
    /// The response is valid but needs something this crate does not do.
    #[error("unsupported: {0}")]
    Unsupported(&'static str),
    /// The head is longer than [`Limits::head`](crate::Limits::head) allows.
    #[error("response head is larger than the limit")]
    HeadTooLarge,
    /// The head has more header fields than [`Limits::headers`](crate::Limits::headers).
    #[error("response has more header fields than the limit")]
    TooManyHeaders,
    /// The body is longer than [`Limits::body`](crate::Limits::body) allows.
    #[error("response body is larger than the limit")]
    BodyTooLarge,
    /// The server sent more interim (1xx) responses than a client should sit through.
    #[error("too many interim responses")]
    TooManyInterimResponses,
    /// The connection ended before the server sent any part of a response. On a reused
    /// connection this usually means the server had already closed it, and a request that is
    /// safe to repeat can be sent again on a new one.
    #[error("connection closed before any response")]
    ClosedBeforeResponse,
    /// The connection ended partway through the response.
    #[error("connection closed partway through the response")]
    Truncated,
    /// A response whose body runs to the end of the connection ended without the transport
    /// confirming a clean close, so the body may be cut short.
    #[error("response body ended without a clean close")]
    UncleanClose,
    /// The call is not allowed now: the response is already complete, or decoding failed.
    #[error("not allowed in the decoder's current state")]
    InvalidState,
}
