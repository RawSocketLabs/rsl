//! The one I/O boundary of the control plane.

// --- Standard library ---
use std::{fmt, io};

// --- Workspace dependencies ---
use http1::{Request, Response};

/// A way to put one HTTPS request to the gateway and get its response.
///
/// This is the seam between the protocol and the network. The login sequence is written
/// against it, and tests script it. A network implementation applies the caller's TLS
/// verification policy before sending the request. Unverified TLS must be chosen explicitly;
/// the protocol's anti-MITM checks cannot authenticate the gateway on their own (spec §2.1).
///
/// Send requests only to their intended gateway and return redirects as responses. Enforce
/// a response body limit and document the scope of any time limit. The built-in network
/// portal limits each connect attempt, read and write. DNS resolution and the total exchange
/// time are unbounded.
pub trait Portal {
    /// Send `request` and return the response, with the certificate chain of the connection
    /// that carried it.
    ///
    /// # Errors
    ///
    /// A request addressed to another gateway, or any failure to connect, complete the TLS
    /// handshake or read a whole response.
    /// An HTTP error status is not a failure: it comes back in the [`Reply`].
    fn exchange(&mut self, request: &Request) -> io::Result<Reply>;
}

/// What a [`Portal`] returns for one request.
#[derive(Clone)]
pub struct Reply {
    /// The HTTP response.
    pub response: Response,
    /// The certificates the server presented on this connection, each in DER, leaf first.
    /// The anti-MITM certificate check reads them (spec §3.5).
    pub peer_chain: Vec<Vec<u8>>,
}

impl fmt::Debug for Reply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Reply")
            .field("status", &self.response.head.status)
            .field("body_len", &self.response.body.len())
            .field("certificates", &self.peer_chain.len())
            .finish_non_exhaustive()
    }
}
