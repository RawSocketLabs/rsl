//! What a caller decides about a connection before it starts.

// --- Internal modules ---
use crate::ServerVerification;

/// The caller's choices for one connection.
#[derive(Clone, Copy, Debug)]
pub struct ClientConfig<'a> {
    /// The host name sent in the `server_name` extension, or `None` to send no such
    /// extension. It is what the server uses to pick a certificate; the name the certificate
    /// is checked against is the one inside [`ServerVerification::Path`].
    pub server_name: Option<&'a str>,
    /// How the server's certificate chain is judged. There is no default.
    pub verification: ServerVerification<'a>,
}
