//! A bounded, parked queue owner; reactor shutdown never joins this thread.

use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::thread::{self, Thread};
use std::time::Instant;

use mio::Waker;
use mio::event::Source;

use super::backend::{Backend, Request, Turn};
use crate::lowlevel::BusError;

/// The single admitted worker command; shutdown has a separate, unsaturable path.
pub(in super::super) enum Work {
    /// Submit without waiting for its USB completion.
    Submit(Request),
    /// Drive bounded reaping after an edge or an exhausted budget.
    Reap { id: u64 },
}

/// The single admitted worker response; no blocking publication on the reactor.
pub(in super::super) enum Report {
    /// Submission succeeded; always request an initial reap to cover an earlier edge.
    Submitted { id: u64 },
    /// Bounded completion work or its original native error.
    Turn {
        id: u64,
        result: Result<Turn, BusError>,
    },
    /// Submission failed before or at the kernel boundary.
    Failed { id: u64, error: BusError },
}

impl Report {
    /// Correlates every acknowledgement/turn/failure with its admitted request.
    pub(in super::super) fn id(&self) -> u64 {
        match self {
            Self::Submitted { id } | Self::Turn { id, .. } | Self::Failed { id, .. } => *id,
        }
    }
}

/// Separate failure and registration-retirement signals cannot be queued out by traffic.
#[derive(Default)]
struct Signals {
    /// Set once the reactor has retired the registration and handed over its source.
    retired: AtomicBool,
    /// The retired readiness, dropped on the worker before cleanup releases the device.
    source: Mutex<Option<Box<dyn Source + Send>>>,
    /// A panic or broken report channel has stopped normal worker service.
    failed: AtomicBool,
}

/// Reactor-side mailbox endpoints, never owning a queue or a claim.
pub(in super::super) struct Worker {
    /// At most one command in addition to the single worker turn executing it.
    commands: mpsc::SyncSender<Work>,
    /// At most one completed worker turn awaits the reactor.
    reports: mpsc::Receiver<Report>,
    /// Cleanup outcome is not competing with ordinary reports for capacity.
    cleanup: mpsc::Receiver<Result<(), BusError>>,
    /// Unparks the worker on command admission or registration retirement.
    thread: Thread,
    /// Shutdown and failure state remain independent of all mailbox occupancy.
    signals: Arc<Signals>,
}

impl Worker {
    /// Transfers complete backend ownership during blocking setup; shares the caller's
    /// existing Waker rather than creating a per-device one.
    pub(in super::super) fn spawn<B: Backend>(
        backend: B,
        wake: Arc<Waker>,
        budget: NonZeroUsize,
    ) -> Result<Self, BusError> {
        let (commands, incoming) = mpsc::sync_channel(1);
        let (outgoing, reports) = mpsc::sync_channel(1);
        let (finished, cleanup) = mpsc::sync_channel(1);
        let signals = Arc::new(Signals::default());
        let worker_signals = Arc::clone(&signals);
        let handle = thread::Builder::new()
            .name("usdr-usb".into())
            .spawn(move || {
                // The backend is outside the unwind boundary: a serving panic cannot release
                // the device while the reactor still has its readiness registered.
                let mut backend = backend;
                let served = catch_unwind(AssertUnwindSafe(|| {
                    Self::serve(
                        &mut backend,
                        &incoming,
                        &outgoing,
                        &worker_signals,
                        &wake,
                        budget,
                    );
                }));
                if served.is_err() {
                    worker_signals.failed.store(true, Ordering::Release);
                    let _ = wake.wake();
                }
                while !worker_signals.retired.load(Ordering::Acquire) {
                    thread::park();
                }
                // The readiness refers to the backend's device, so it goes first.
                drop(
                    worker_signals
                        .source
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .take(),
                );
                let cleaned = catch_unwind(AssertUnwindSafe(|| backend.cleanup()))
                    .unwrap_or(Err(BusError::Disconnected));
                let dropped = catch_unwind(AssertUnwindSafe(|| drop(backend)));
                let result = if dropped.is_err() {
                    Err(BusError::Disconnected)
                } else {
                    cleaned
                };
                let _ = finished.try_send(result);
                let _ = wake.wake();
            })
            .map_err(BusError::from)?;
        let thread = handle.thread().clone();
        // Detach, never join: a kernel call or cleanup can outlive the reactor/observer.
        drop(handle);
        Ok(Self {
            commands,
            reports,
            cleanup,
            thread,
            signals,
        })
    }

    /// Services one request at a time; empty mailboxes park, not poll or sleep.
    fn serve<B: Backend>(
        backend: &mut B,
        commands: &mpsc::Receiver<Work>,
        reports: &mpsc::SyncSender<Report>,
        signals: &Signals,
        wake: &Waker,
        budget: NonZeroUsize,
    ) {
        while !signals.retired.load(Ordering::Acquire) {
            let command = match commands.try_recv() {
                Ok(command) => command,
                Err(mpsc::TryRecvError::Empty) => {
                    thread::park();
                    continue;
                }
                Err(mpsc::TryRecvError::Disconnected) => break,
            };
            // Retirement wins over an already-admitted ordinary command.
            if signals.retired.load(Ordering::Acquire) {
                break;
            }
            let report = match command {
                Work::Submit(request) => {
                    let id = request.id;
                    if Instant::now() >= request.expires {
                        Report::Failed {
                            id,
                            error: BusError::Timeout,
                        }
                    } else {
                        match backend.submit(request) {
                            Ok(()) => Report::Submitted { id },
                            Err(error) => Report::Failed { id, error },
                        }
                    }
                }
                Work::Reap { id } => Report::Turn {
                    id,
                    result: backend.turn(budget),
                },
            };
            if reports.try_send(report).is_err() {
                signals.failed.store(true, Ordering::Release);
                let _ = wake.wake();
                break;
            }
            let _ = wake.wake();
        }
    }

    /// Admits without waiting for capacity; a full slot is a coordinator invariant error.
    pub(in super::super) fn send(&self, work: Work) -> Result<(), BusError> {
        if self.signals.failed.load(Ordering::Acquire) {
            return Err(BusError::Disconnected);
        }
        self.commands
            .try_send(work)
            .map_err(|_| BusError::Disconnected)?;
        self.thread.unpark();
        Ok(())
    }

    /// Observes at most one response, including a worker panic with no report.
    pub(in super::super) fn report(&self) -> Result<Option<Report>, BusError> {
        if self.signals.failed.load(Ordering::Acquire) {
            return Err(BusError::Disconnected);
        }
        match self.reports.try_recv() {
            Ok(report) => Ok(Some(report)),
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(mpsc::TryRecvError::Disconnected) => Err(BusError::Disconnected),
        }
    }

    /// Hands over the retired readiness and lets the worker clean up and release the device.
    pub(in super::super) fn retire(&self, source: Box<dyn Source + Send>) {
        *self
            .signals
            .source
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(source);
        self.signals.retired.store(true, Ordering::Release);
        self.thread.unpark();
    }

    /// Observes cleanup, without blocking or joining; consumed exactly once by its caller.
    pub(in super::super) fn cleanup(&self) -> Option<Result<(), BusError>> {
        match self.cleanup.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(BusError::Disconnected)),
        }
    }
}
