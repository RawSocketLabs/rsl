use super::{Client, Protocol};
use crate::{Destination, Stream, error::Error, v5};
use std::net::SocketAddr;

impl Client {
    /// Perform a blocking handshake on a caller-supplied transport and deadlines.
    ///
    /// # Errors
    /// Invalid local destinations fail before I/O; handshake failures are terminal.
    pub fn connect<S: std::io::Read + std::io::Write>(
        &self,
        stream: S,
        destination: Destination,
    ) -> Result<(Stream<S>, Destination), Error> {
        let (stream, bound) = match &self.protocol {
            Protocol::V5(config) => {
                v5::client::blocking::connect(stream, destination.into(), config.authentication())?
            }
        };
        Ok((stream, bound.into()))
    }

    /// Connect to a numeric proxy and complete a blocking handshake within the TCP budget.
    ///
    /// # Errors
    /// Returns a terminal connection, timeout, or handshake error.
    pub fn connect_tcp(
        &self,
        proxy: SocketAddr,
        destination: Destination,
    ) -> Result<(Stream<std::net::TcpStream>, Destination), Error> {
        let (stream, bound) = match &self.protocol {
            Protocol::V5(config) => v5::client::blocking::connect_tcp(
                proxy,
                destination.into(),
                config.authentication(),
                self.timeout,
            )?,
        };
        Ok((stream, bound.into()))
    }
}
