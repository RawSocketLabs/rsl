// --- Standard library ---
use std::io::{Read, Write};
use std::net::SocketAddr;

// --- Internal modules ---
use super::super::{Client as Settings, Configuration, Connection};
use super::Handshake;
use crate::v5::client::mio::{Client as V5Handshake, connect_tcp as connect_v5};
use crate::{Destination, Version, error::Error};

/// Reusable Mio client with validated version-specific settings.
/// Connected streams and in-progress handshakes are owned separately from this configuration.
pub struct Client {
    pub(super) settings: Settings,
}

impl Client {
    /// The selected protocol version, with no automatic fallback.
    #[must_use]
    pub const fn version(&self) -> Version {
        self.settings.version()
    }

    /// Start a caller-driven handshake over an already-connected nonblocking transport.
    /// Complete TCP establishment with `io::mio::Connector` first. The caller owns
    /// readiness registration, fairness continuation, and the absolute deadline;
    /// the configured TCP timeout does not schedule or expire this handshake.
    ///
    /// # Errors
    /// Invalid local arguments fail before any protocol I/O.
    pub fn connect_with<S: Read + Write>(
        &self,
        stream: S,
        dest: Destination,
    ) -> Result<Handshake<S>, Error> {
        match &self.settings.configuration {
            Configuration::V5(cfg) => Ok(Handshake::new(V5Handshake::new(
                stream,
                dest.into(),
                cfg.auth(),
            )?)),
        }
    }

    /// Run the Mio convenience poll loop within the TCP budget. Retain and reuse
    /// the returned original poll for the stream's lifetime; do not move it to another poll.
    /// Native-loop users use [`Self::connect_with`] after completing TCP establishment.
    ///
    /// # Errors
    /// Returns a terminal connection, timeout, registration, or handshake error.
    pub fn connect_blocking(
        &self,
        proxy: SocketAddr,
        dest: Destination,
    ) -> Result<(mio::Poll, Connection<mio::net::TcpStream>), Error> {
        let (poll, stream, bound) = match &self.settings.configuration {
            Configuration::V5(cfg) => {
                connect_v5(proxy, dest.into(), cfg.auth(), self.settings.timeout)?
            }
        };
        Ok((poll, Connection::new(stream, bound.into())))
    }
}
