// --- Standard library ---
use std::net::SocketAddr;

// --- Internal modules ---
use super::super::{Client as Settings, Configuration, Connection};
use crate::v5::client::blocking::{connect as connect_v5, connect_with as connect_with_v5};
use crate::{Destination, Version, error::Error};

/// Reusable blocking client with validated version-specific settings.
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

    /// Perform a blocking handshake on an already-connected transport with caller-owned deadlines.
    ///
    /// # Errors
    /// Invalid local destinations fail before I/O; handshake failures are terminal.
    pub fn connect_with<S: std::io::Read + std::io::Write>(
        &self,
        stream: S,
        dest: Destination,
    ) -> Result<Connection<S>, Error> {
        let (stream, bound) = match &self.settings.configuration {
            Configuration::V5(cfg) => connect_with_v5(stream, dest.into(), cfg.auth())?,
        };
        Ok(Connection::new(stream, bound.into()))
    }

    /// Connect to a numeric proxy and complete a blocking handshake within the TCP budget.
    ///
    /// # Errors
    /// Returns a terminal connection, timeout, or handshake error.
    pub fn connect(
        &self,
        proxy: SocketAddr,
        dest: Destination,
    ) -> Result<Connection<std::net::TcpStream>, Error> {
        let (stream, bound) = match &self.settings.configuration {
            Configuration::V5(cfg) => {
                connect_v5(proxy, dest.into(), cfg.auth(), self.settings.timeout)?
            }
        };
        Ok(Connection::new(stream, bound.into()))
    }
}
