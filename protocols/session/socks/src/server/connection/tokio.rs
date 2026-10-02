//! Asynchronous half-close-aware relay on an established server connection.
// --- Standard library ---
use std::time::Duration;

// --- Workspace dependencies ---
use tokio::net::TcpStream;

// --- Internal modules ---
use super::Connection;
use crate::{error::Error, io::tokio::bounded};

impl Connection<TcpStream> {
    /// Relay both directions with half-close support and a total lifetime limit.
    /// Cancellation, I/O failure, or an expired deadline closes both owned sides.
    ///
    /// # Errors
    /// Returns invalid limits, I/O, or timeout errors.
    pub async fn relay(mut self, timeout: Duration) -> Result<(), Error> {
        bounded(timeout, async {
            tokio::io::copy_bidirectional(&mut self.client, &mut self.target).await?;
            Ok(())
        })
        .await
    }
}
