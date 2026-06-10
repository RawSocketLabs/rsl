use std::net::{SocketAddr, TcpStream};

use crate::client::client::{read_reply, reply_socket_addr};
use crate::error::Result;

/// A pending BIND command (RFC 1928).
///
/// The proxy is listening on [`BindListener::bound`]; the expected peer
/// should connect there. Call [`BindListener::accept`] to wait for it.
pub struct BindListener {
    stream: TcpStream,
    /// The address the proxy is listening on.
    pub bound: SocketAddr,
}

impl BindListener {
    pub(crate) fn new(stream: TcpStream, bound: SocketAddr) -> Self {
        Self { stream, bound }
    }

    /// Blocks until the expected peer connects to the proxy, returning the
    /// now-transparent stream and the peer's address.
    ///
    /// # Errors
    /// Returns [`SocksError::ReplyFailure`](crate::error::SocksError::ReplyFailure)
    /// when the proxy reports a failure, or an I/O or parse error.
    pub fn accept(self) -> Result<(TcpStream, SocketAddr)> {
        let reply = read_reply(&self.stream)?;
        let peer = reply_socket_addr(&reply)?;
        Ok((self.stream, peer))
    }
}
