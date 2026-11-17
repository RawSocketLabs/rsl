//! The one I/O boundary of the control plane.

// --- Standard library ---
use std::io;

// --- Workspace dependencies ---
use http1::{Request, Response};

/// A way to put one HTTPS request to the gateway and get its response.
///
/// This is the seam between the protocol and the network. The login sequence is written
/// against it, and tests script it. What an implementation owes:
///
/// - It connects over TLS without verifying the certificate (spec §2.1): gateways use
///   self-signed ones, and the protocol's own anti-MITM checks read the chain instead.
/// - It sends `request` as it is.
/// - It follows no redirect: a 3xx answer comes back like any other. The reference client's
///   HTTP library follows them (spec §2.2); no request of the catalogue is known to be
///   redirected, and the readers here report one as a failure.
/// - It bounds the response body (the readers assume about a megabyte at most; a policy
///   document may need more) and the time the whole exchange may take (20 s in the
///   reference, spec §2.2).
pub trait Portal {
    /// Send `request` and return the response, with the certificate chain of the connection
    /// that carried it.
    ///
    /// # Errors
    ///
    /// Any failure to connect, to complete the TLS handshake, or to read a whole response.
    /// An HTTP error status is not a failure: it comes back in the [`Reply`].
    fn exchange(&mut self, request: &Request) -> io::Result<Reply>;
}

/// What a [`Portal`] returns for one request.
#[derive(Clone, Debug)]
pub struct Reply {
    /// The HTTP response.
    pub response: Response,
    /// The certificates the server presented on this connection, each in DER, leaf first.
    /// The anti-MITM certificate check reads them (spec §3.5).
    pub peer_chain: Vec<Vec<u8>>,
}
