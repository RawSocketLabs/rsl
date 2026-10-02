// --- Workspace dependencies ---
use tokio::{net::TcpListener, runtime::Handle};

// --- Internal modules ---
use super::{Running, Shutdown, event::Events, listener::Listener};
use crate::{error::Error, server::tokio::Server};

/// An owned listener and server configuration, not yet accepting sessions.
#[must_use = "call start to begin serving"]
pub struct Proxy {
    listener: TcpListener,
    server: Server,
}

impl Proxy {
    /// Transfer ownership without spawning tasks or accepting connections.
    pub(crate) fn new(listener: TcpListener, server: Server) -> Self {
        Self { listener, server }
    }

    /// Start serving independently of event consumption on the current Tokio runtime.
    /// The listener's I/O runtime must remain alive; policy callbacks must not block.
    /// Retains at most 64 session errors, counting overflow separately via `Running::take_dropped_count`.
    ///
    /// # Errors
    /// Returns `Error::TokioRuntimeUnavailable` if called outside a Tokio runtime.
    pub fn start(self) -> Result<Running, Error> {
        let runtime = Handle::try_current().map_err(|_| Error::TokioRuntimeUnavailable)?;
        let (shutdown, requested) = Shutdown::new();
        let (events, reporter) = Events::new();
        let task = runtime.spawn(async move {
            Listener::new(self.listener)
                .serve(&self.server, requested, reporter)
                .await
        });
        Ok(Running::new(task, shutdown, events))
    }
}
