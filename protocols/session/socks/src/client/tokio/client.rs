// --- Standard library ---
use std::net::SocketAddr;

// --- Internal modules ---
use super::super::{Client as Settings, Configuration, Connection};
use crate::v5::client::tokio::{connect as connect_v5, connect_with as connect_with_v5};
use crate::{Destination, Version, error::Error};

/// Reusable Tokio client with validated version-specific settings.
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

    /// Perform a Tokio handshake on an already-connected transport with caller-managed deadlines.
    ///
    /// # Errors
    /// Invalid destinations fail before I/O. Errors or cancellation are terminal.
    pub async fn connect_with<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>(
        &self,
        stream: S,
        dest: Destination,
    ) -> Result<Connection<S>, Error> {
        let (stream, bound) = match &self.settings.configuration {
            Configuration::V5(cfg) => connect_with_v5(stream, dest.into(), cfg.auth()).await?,
        };
        Ok(Connection::new(stream, bound.into()))
    }

    /// Connect to a numeric proxy and complete a Tokio handshake within the TCP budget.
    ///
    /// # Errors
    /// Returns connection, timeout, or handshake errors; cancellation is terminal.
    pub async fn connect(
        &self,
        proxy: SocketAddr,
        dest: Destination,
    ) -> Result<Connection<tokio::net::TcpStream>, Error> {
        let (stream, bound) = match &self.settings.configuration {
            Configuration::V5(cfg) => {
                connect_v5(proxy, dest.into(), cfg.auth(), self.settings.timeout).await?
            }
        };
        Ok(Connection::new(stream, bound.into()))
    }
}
