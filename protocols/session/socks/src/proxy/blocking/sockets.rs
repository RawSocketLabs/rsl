//! Socket ownership shared between a session worker and listener shutdown.
// --- Standard library ---
use std::{
    io,
    net::{Shutdown, TcpStream},
};

// --- Internal modules ---
use crate::error::Error;

/// Cancellation closes both sockets, including a target registered after shutdown starts.
pub(super) struct Sockets {
    client: TcpStream,
    target: Option<TcpStream>,
    cancelled: bool,
}

impl Sockets {
    /// Retain a client handle so shutdown can interrupt a worker's I/O.
    pub(super) fn new(client: TcpStream) -> Self {
        Self {
            client,
            target: None,
            cancelled: false,
        }
    }

    /// Register the target before success, or close it if cancellation already won.
    pub(super) fn register(&mut self, target: &TcpStream) -> Result<(), Error> {
        if self.cancelled {
            target.shutdown(Shutdown::Both)?;
            return Err(io::Error::from(io::ErrorKind::Interrupted).into());
        }
        self.target = Some(target.try_clone()?);
        Ok(())
    }

    /// Mark cancellation before closing both retained sockets.
    pub(super) fn shutdown(&mut self) {
        self.cancelled = true;
        let _ = self.client.shutdown(Shutdown::Both);
        if let Some(target) = &self.target {
            let _ = target.shutdown(Shutdown::Both);
        }
    }
}
