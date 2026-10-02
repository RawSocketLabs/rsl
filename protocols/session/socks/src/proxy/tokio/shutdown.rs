// --- Workspace dependencies ---
use tokio::sync::watch;

/// Cloneable, idempotent shutdown request for an owned Tokio proxy.
#[derive(Clone)]
pub struct Shutdown {
    sender: watch::Sender<bool>,
}

impl Shutdown {
    /// Create a request handle and its listener-side notification receiver.
    pub(super) fn new() -> (Self, watch::Receiver<bool>) {
        let (sender, receiver) = watch::channel(false);
        (Self { sender }, receiver)
    }

    /// Stop accepting and abort active sessions; await `Running::join` for cleanup.
    /// Requests made before the listener starts polling are retained.
    pub fn request(&self) {
        self.sender.send_replace(true);
    }

    /// Wait for a retained request or the disappearance of every request handle.
    pub(super) async fn wait(mut receiver: watch::Receiver<bool>) {
        while !*receiver.borrow_and_update() {
            if receiver.changed().await.is_err() {
                break;
            }
        }
    }
}
