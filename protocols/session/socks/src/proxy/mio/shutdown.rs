use mio::Waker;
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// Cross-thread cancellation for an owned proxy poll loop.
#[derive(Clone)]
pub struct Shutdown {
    pub(super) stopped: Arc<AtomicBool>,
    pub(super) waker: Arc<Waker>,
}

impl Shutdown {
    /// Request shutdown and wake a blocked poll. Active sockets close on its next turn.
    ///
    /// # Errors
    /// Returns a platform wake error; the shutdown flag remains set.
    pub fn shutdown(&self) -> io::Result<()> {
        self.stopped.store(true, Ordering::Release);
        self.waker.wake()
    }
}
