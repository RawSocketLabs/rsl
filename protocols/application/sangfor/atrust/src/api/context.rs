//! What every request needs from the session.

// --- Workspace dependencies ---
use sangfor_atrust_sig::SignKey;

// --- Internal modules ---
use super::Gateway;

/// The session values a [`Call`](super::Call) is completed with when it becomes a request.
#[derive(Clone, Copy, Debug)]
pub struct Context<'a> {
    /// The gateway the request goes to.
    pub gateway: &'a Gateway,
    /// The latest CSRF token, empty before the first `authConfig` answer.
    pub csrf_token: &'a str,
    /// The trace id of this request: eight lowercase hex digits, fresh for each (spec §3.8).
    pub trace_id: &'a str,
    /// The sign key from the most recent `authConfig` answer, once there is one.
    pub sign_key: Option<&'a SignKey>,
    /// The `Cookie` header value for this request's path, when the jar has cookies for it.
    pub cookie: Option<&'a [u8]>,
}
