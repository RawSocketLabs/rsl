// --- Workspace dependencies ---
use tokio::task::JoinHandle;

// --- Internal modules ---
use super::{Shutdown, event::Events};
use crate::error::Error;

/// Owned running service, cancellation handle, and bounded diagnostic receiver.
/// Dropping requests shutdown but does not await cleanup; keep the runtime alive and
/// call `join` when completion matters. Shutdown aborts sessions rather than draining traffic.
#[must_use = "retain the running service; dropping it requests shutdown"]
pub struct Running {
    task: Option<JoinHandle<Result<(), Error>>>,
    shutdown: Shutdown,
    events: Events,
}

impl Running {
    /// Retain the spawned listener and both lifecycle channels.
    pub(super) fn new(
        task: JoinHandle<Result<(), Error>>,
        shutdown: Shutdown,
        events: Events,
    ) -> Self {
        Self {
            task: Some(task),
            shutdown,
            events,
        }
    }

    /// Obtain an independently owned handle for requesting shutdown.
    #[must_use]
    pub fn shutdown_handle(&self) -> Shutdown {
        self.shutdown.clone()
    }

    /// Wait for a retained session failure without driving the service.
    /// Errors retain FIFO order. Cancellation does not consume an error. `None`
    /// means reporting ended, not that cleanup succeeded: call `join` for status.
    /// Receiving never resets the separate loss counter. Consume errors until EOF,
    /// then sample losses and join, or request shutdown and join without consuming.
    pub async fn next_session_error(&mut self) -> Option<Error> {
        self.events.next_session_error().await
    }

    /// Retrieve and reset the number of diagnostics dropped because the queue was full.
    /// The count saturates at `usize::MAX`; zero means no losses since the last sample.
    /// Sampling never consumes queued errors and remains valid after EOF or joining.
    /// Concurrent losses fall into this sample or a later one, without double counting.
    /// Choose a batch or timer cadence in application code; sample once more at EOF.
    #[must_use]
    pub fn take_dropped_count(&self) -> usize {
        self.events.take_dropped_count()
    }

    /// Wait for service termination and session cleanup, without requesting shutdown.
    /// Does not require event consumption and is cancellation-safe: a later call can
    /// resume waiting. Buffered diagnostics remain available after completion.
    /// System resolver work already running in Tokio's blocking pool may outlive sessions.
    /// Shutdown has priority over simultaneously ready failures; completions during
    /// shutdown cleanup are discarded.
    ///
    /// # Errors
    /// Returns a listener error or `WorkerPanicked` observed before shutdown wins.
    /// Failure of the service task itself also returns `WorkerPanicked`.
    /// A second completed join returns `InvalidState`.
    pub async fn join(&mut self) -> Result<(), Error> {
        let result = self.task.as_mut().ok_or(Error::InvalidState)?.await;
        self.task = None;
        result.map_err(|_| Error::WorkerPanicked)?
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.shutdown.request();
    }
}
