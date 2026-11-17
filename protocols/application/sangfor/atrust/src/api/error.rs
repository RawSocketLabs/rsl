//! What can go wrong building a request or reading an answer.

// --- Workspace dependencies ---
use thiserror::Error as ThisError;

/// A request that could not be built, or an answer that could not be used.
#[derive(Debug, ThisError)]
#[non_exhaustive]
pub enum Error {
    /// A value that has to travel in the request line or a header cannot appear there.
    #[error("the request cannot be built")]
    Request(#[source] http1::Error),
    /// A signed request was asked for before any sign key was established.
    #[error("the request must be signed and no sign key is established")]
    NoSignKey,
    /// A primitive of the protocol refused its input.
    #[error("a protocol primitive failed")]
    Sig(#[from] sangfor_atrust_sig::Error),
    /// The response does not have the status the endpoint requires.
    #[error("the gateway answered with HTTP status {0}")]
    Status(u16),
    /// The body is not JSON, or a member the endpoint reads has the wrong type.
    #[error("the gateway's answer is not the JSON this endpoint returns")]
    Json(#[source] serde_json::Error),
    /// The gateway no longer knows the session: a full login is needed (codes 10000004 and
    /// 75500002, or status 401 or 403 where the specification says so).
    #[error("the session is not valid (code {code})")]
    SessionInvalid {
        /// The code the gateway gave, or the HTTP status when it gave none.
        code: i64,
    },
    /// The gateway refused the request with a code other than zero.
    #[error("the gateway refused the request: {code}: {message}")]
    Refused {
        /// The gateway's code.
        code: i64,
        /// The gateway's message, which may be empty.
        message: String,
    },
}
