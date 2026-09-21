use super::{Client, Protocol};
use crate::{Destination, Stream, error::Error, v5};
use std::net::SocketAddr;

impl Client {
    /// Perform a Tokio handshake on a transport with caller-managed deadlines.
    ///
    /// # Errors
    /// Invalid destinations fail before I/O. Errors or cancellation are terminal.
    pub async fn connect_async<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
        &self,
        stream: S,
        destination: Destination,
    ) -> Result<(Stream<S>, Destination), Error> {
        let (stream, bound) = match &self.protocol {
            Protocol::V5(config) => {
                v5::client::tokio::connect(stream, destination.into(), config.authentication())
                    .await?
            }
        };
        Ok((stream, bound.into()))
    }

    /// Connect to a numeric proxy and complete a Tokio handshake within the TCP budget.
    ///
    /// # Errors
    /// Returns connection, timeout, or handshake errors; cancellation is terminal.
    pub async fn connect_tcp_async(
        &self,
        proxy: SocketAddr,
        destination: Destination,
    ) -> Result<(Stream<tokio::net::TcpStream>, Destination), Error> {
        let (stream, bound) = match &self.protocol {
            Protocol::V5(config) => {
                v5::client::tokio::connect_tcp(
                    proxy,
                    destination.into(),
                    config.authentication(),
                    self.timeout,
                )
                .await?
            }
        };
        Ok((stream, bound.into()))
    }
}
