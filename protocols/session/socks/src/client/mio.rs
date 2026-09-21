use super::{Client, Protocol};
use crate::{Destination, Stream, error::Error, v5};
use std::net::SocketAddr;

impl Client {
    /// Run the Mio convenience poll loop within the TCP budget. Retain and reuse
    /// the returned original poll for the stream's lifetime; do not move it to another poll.
    /// Native-loop users use [`v5::client::mio::Client`] with version configuration.
    ///
    /// # Errors
    /// Returns a terminal connection, timeout, registration, or handshake error.
    pub fn connect_tcp_mio(
        &self,
        proxy: SocketAddr,
        destination: Destination,
    ) -> Result<(mio::Poll, Stream<mio::net::TcpStream>, Destination), Error> {
        let (poll, stream, bound) = match &self.protocol {
            Protocol::V5(config) => v5::client::mio::connect_tcp(
                proxy,
                destination.into(),
                config.authentication(),
                self.timeout,
            )?,
        };
        Ok((poll, stream, bound.into()))
    }
}
