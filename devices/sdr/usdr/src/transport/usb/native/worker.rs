//! A bounded, parked queue owner; reactor shutdown never joins this thread. It runs every
//! USB call: control transfers, reap turns, and the RX/TX streams' opening, block copies,
//! sends and cancellation.

use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::thread::{self, Thread};
use std::time::{Duration, Instant};

use mio::Waker;
use mio::event::Source;

use super::backend::{Backend, Completion, Filing, Request, Turn};
use super::handle::RxBlock;
use crate::lowlevel::BusError;

/// Worker commands the adapter may have outstanding: one operation's command (a control
/// submission or a stream command) and one reap turn. Each is answered by one report.
pub(in super::super) const MAILBOX: usize = 2;

/// An admitted worker command; shutdown has a separate, unsaturable path.
#[expect(
    clippy::large_enum_variant,
    reason = "one command in flight; its inline frame spares an allocation per transfer"
)]
pub(in super::super) enum Work {
    /// Submit without waiting for its USB completion.
    Submit(Request),
    /// Drive bounded reaping after an edge or an exhausted budget, then take up to `rx`
    /// filed RX blocks and, for the send of generation `acquire`, try for a free TX
    /// transfer.
    Turn { rx: usize, acquire: Option<u64> },
    /// Open, close or send on a stream; may block (cancellation) on the worker.
    Stream(StreamCommand),
}

/// A stream command, answered by [`Report::Stream`].
pub(in super::super) enum StreamCommand {
    /// Opens RX for blocks of this many bytes and primes its ring.
    RxOpen(u32),
    /// Cancels RX and releases its endpoint.
    RxClose,
    /// Opens TX for blocks of up to `block_bytes`, samples of `sample_bytes`.
    TxOpen { block_bytes: u32, sample_bytes: u32 },
    /// Frames `block` with `time` into the acquired transfer and queues it.
    TxCommit {
        time: Option<u64>,
        status: [u32; 4],
        block: Vec<u8>,
    },
    /// Cancels TX and releases its endpoint.
    TxClose,
}

impl StreamCommand {
    /// How long the worker may take over this command before the handle takes it for
    /// stuck in the kernel and ends the session. An open allocates and queues transfers
    /// and a commit queues one; a close cancels every transfer and waits up to the `usb`
    /// queues' own 5 s grace for them, so it gets twice that.
    pub(in super::super) const fn deadline(&self) -> Duration {
        match self {
            Self::RxOpen(_) | Self::TxOpen { .. } | Self::TxCommit { .. } => Duration::from_secs(2),
            Self::RxClose | Self::TxClose => Duration::from_secs(10),
        }
    }
}

/// What one worker turn collected: no kernel-owned memory, and RX blocks already copied.
pub(in super::super) struct Harvest {
    /// The control request's completion, if it finished.
    pub completion: Option<Completion>,
    /// RX blocks taken, oldest first; an error ends the list and the stream.
    pub rx: Vec<Result<RxBlock, BusError>>,
    /// The TX acquisition attempt, if asked for, with the send generation it was for:
    /// whether a free transfer is now held.
    pub acquired: Option<(u64, Result<bool, BusError>)>,
    /// Another turn is due without a new readiness edge.
    pub exhausted: bool,
}

/// An admitted worker response; no blocking publication on the reactor.
pub(in super::super) enum Report {
    /// Submission succeeded; always request an initial reap to cover an earlier edge.
    Submitted { id: u64 },
    /// Bounded completion work or its original native error.
    Turn(Result<Harvest, BusError>),
    /// Submission failed before or at the kernel boundary.
    Failed { id: u64, error: BusError },
    /// A stream command's result.
    Stream(Result<(), BusError>),
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
    /// At most [`MAILBOX`] admitted commands, in order.
    commands: mpsc::SyncSender<Work>,
    /// At most [`MAILBOX`] reports await the reactor, one per command.
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
        let (commands, incoming) = mpsc::sync_channel(MAILBOX);
        let (outgoing, reports) = mpsc::sync_channel(MAILBOX);
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
                Work::Turn { rx, acquire } => {
                    Report::Turn(Self::harvest(backend, budget, rx, acquire))
                }
                Work::Stream(command) => Report::Stream(Self::stream(backend, command)),
            };
            if reports.try_send(report).is_err() {
                signals.failed.store(true, Ordering::Release);
                let _ = wake.wake();
                break;
            }
            let _ = wake.wake();
        }
    }

    /// One bounded turn: reaps and takes the control completion, then copies up to `rx`
    /// filed RX blocks (each transfer requeued as it is copied) and, for the send of
    /// generation `acquire`, tries for a free TX transfer. Stream work stops at the first
    /// failure; a reap failure skips it.
    fn harvest<B: Backend>(
        backend: &mut B,
        budget: NonZeroUsize,
        rx: usize,
        acquire: Option<u64>,
    ) -> Result<Harvest, BusError> {
        let Turn {
            completion,
            mut exhausted,
        } = backend.turn(budget)?;
        let mut blocks = Vec::new();
        while blocks.len() < rx {
            let mut block = None;
            let taken = backend.rx_receive(&mut |payload, oob| {
                block = Some(RxBlock {
                    payload: payload.to_vec(),
                    oob,
                });
            });
            match taken {
                Ok(Filing::Taken) => blocks.push(Ok(
                    block.expect("invariant: a taken RX block was lent before its requeue")
                )),
                Ok(filing) => {
                    exhausted |= filing == Filing::Again;
                    break;
                }
                Err(error) => {
                    blocks.push(Err(error));
                    break;
                }
            }
        }
        // Blocks may still be filed beyond the room the reactor gave this turn.
        exhausted |= rx > 0 && blocks.len() == rx && blocks.last().is_some_and(Result::is_ok);
        let acquired = acquire.map(|generation| {
            let acquired = match backend.tx_acquire() {
                Ok(filing) => {
                    exhausted |= filing == Filing::Again;
                    Ok(filing == Filing::Taken)
                }
                Err(error) => Err(error),
            };
            (generation, acquired)
        });
        Ok(Harvest {
            completion,
            rx: blocks,
            acquired,
            exhausted,
        })
    }

    /// Runs one stream command.
    fn stream<B: Backend>(backend: &mut B, command: StreamCommand) -> Result<(), BusError> {
        match command {
            StreamCommand::RxOpen(block_bytes) => backend.rx_open(block_bytes),
            StreamCommand::RxClose => backend.rx_close(),
            StreamCommand::TxOpen {
                block_bytes,
                sample_bytes,
            } => backend.tx_open(block_bytes, sample_bytes),
            StreamCommand::TxCommit {
                time,
                status,
                block,
            } => backend.tx_commit(time, status, &mut |buffer, _| {
                // A block longer than the transfer is reported back whole, so the shared
                // `Tx::commit` refuses it rather than this copy truncating it.
                if let Some(target) = buffer.get_mut(..block.len()) {
                    target.copy_from_slice(&block);
                }
                block.len()
            }),
            StreamCommand::TxClose => backend.tx_close(),
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
