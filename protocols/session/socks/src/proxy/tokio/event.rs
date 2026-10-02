// --- Standard library ---
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

// --- Workspace dependencies ---
use tokio::sync::mpsc::{self, Receiver, Sender, error::TrySendError};

// --- Internal modules ---
use crate::error::Error;

/// Bounded session errors and an independently sampled loss counter.
pub(super) struct Events {
    receiver: Receiver<Error>,
    dropped: Arc<AtomicUsize>,
}

impl Events {
    /// Bound retained errors to 64 regardless of event-consumer progress.
    pub(super) fn new() -> (Self, Reporter) {
        let (sender, receiver) = mpsc::channel(64);
        let dropped = Arc::new(AtomicUsize::new(0));

        let reporter = Reporter::new(sender, Arc::clone(&dropped));
        let events = Self { receiver, dropped };

        (events, reporter)
    }

    /// Wait for one retained session error without touching the loss counter.
    pub(super) async fn next_session_error(&mut self) -> Option<Error> {
        self.receiver.recv().await
    }

    /// Atomically take losses since the preceding sample, saturating at `usize::MAX`.
    pub(super) fn take_dropped_count(&self) -> usize {
        self.dropped.swap(0, Ordering::Relaxed)
    }
}

/// Nonblocking producer used directly by the listener for session errors.
pub(super) struct Reporter {
    sender: Sender<Error>,
    dropped: Arc<AtomicUsize>,
}

impl Reporter {
    /// Pair the queue sender with its shared loss counter.
    fn new(sender: Sender<Error>, dropped: Arc<AtomicUsize>) -> Self {
        Self { sender, dropped }
    }

    /// Never wait for diagnostics; coalesce losses when the queue is full.
    pub(super) fn report(&self, error: Error) {
        if matches!(self.sender.try_send(error), Err(TrySendError::Full(_))) {
            let _ = self
                .dropped
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
                    Some(count.saturating_add(1))
                });
        }
    }
}

#[cfg(test)]
mod unit {
    // --- Internal modules ---
    use super::Events;
    use crate::error::Error;

    #[tokio::test]
    async fn loss_sampling_is_independent_of_fifo_delivery_and_resets() {
        let (mut events, reporter) = Events::new();
        for code in 0..64 {
            reporter.report(Error::VersionNotAccepted(code));
        }
        for _ in 0..3 {
            reporter.report(Error::InvalidState);
        }
        assert_eq!(events.take_dropped_count(), 3);
        assert_eq!(events.take_dropped_count(), 0);
        reporter.report(Error::InvalidState);

        // Receiving an error neither reports nor clears the pending loss count.
        assert!(matches!(
            events.next_session_error().await,
            Some(Error::VersionNotAccepted(0))
        ));
        reporter.report(Error::PermissionDenied);
        drop(reporter);
        for expected in 1..64 {
            assert!(matches!(events.next_session_error().await,
                Some(Error::VersionNotAccepted(actual)) if actual == expected));
        }
        assert!(matches!(
            events.next_session_error().await,
            Some(Error::PermissionDenied)
        ));
        assert!(events.next_session_error().await.is_none());
        assert_eq!(
            events.take_dropped_count(),
            1,
            "EOF must not discard unsampled losses"
        );
        assert_eq!(events.take_dropped_count(), 0);
    }

    #[tokio::test]
    async fn cancelling_receive_does_not_consume_the_next_error() {
        let (mut events, reporter) = Events::new();
        tokio::select! { biased;
            _ = events.next_session_error() => panic!("empty open channel must wait"),
            () = std::future::ready(()) => {}
        }
        reporter.report(Error::PermissionDenied);
        assert!(matches!(
            events.next_session_error().await,
            Some(Error::PermissionDenied)
        ));
        assert_eq!(events.take_dropped_count(), 0);
    }

    #[tokio::test]
    async fn closed_consumer_never_blocks_reporting() {
        let (events, reporter) = Events::new();
        drop(events);
        reporter.report(Error::InvalidState);
    }
}
