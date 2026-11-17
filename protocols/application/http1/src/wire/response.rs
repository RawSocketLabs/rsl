//! A whole response.

// --- Internal modules ---
use super::ResponseHead;

/// One complete response: its head, its decoded body, and what it says about the connection.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Response {
    /// The status line and header fields.
    pub head: ResponseHead,
    /// The body with any transfer coding removed. Empty when there is none.
    pub body: Vec<u8>,
    /// Whether the server left the connection usable for another request: an HTTP/1.1
    /// response, with no `Connection: close`, whose body ended on its own (RFC 9112 §9.3).
    /// A caller that asked for the connection to be closed should ignore this.
    pub keep_alive: bool,
}
