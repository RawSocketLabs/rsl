//! Registration, deadlines and coalesced worker scheduling; no USB syscalls here.

use std::io;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Instant;

use mio::event::Source;
use mio::{Interest, Registry, Token, Waker};

use super::super::machine::{Submission, Transfer};
use super::backend::{Backend, Completion, Request};
use super::worker::{Report, Work, Worker};
use crate::lowlevel::BusError;

/// Single-request native executor registered in an externally owned Poll.
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent admission, mailbox, edge and cleanup-publication facts"
)]
pub(in super::super) struct Adapter {
    /// Cloned registration control, not an internally owned event loop.
    registry: Registry,
    /// The device's readiness until explicit successful deregistration; errors remain
    /// retryable. Dropped before the worker retires, so the worker releases the device.
    source: Option<Box<dyn Source + Send>>,
    /// Caller-owned session token; do not reuse until this registration is retired.
    token: Token,
    /// Failure/explicit close stops admission even if deregistration needs retry.
    stopped: bool,
    /// Automatic retirement failure, separate from the original operation result.
    retirement_error: Option<io::Error>,
    /// Backend owner and bounded mailboxes; Drop never joins it.
    worker: Worker,
    /// The shared Waker, kept open for the worker's final publication: closing its last
    /// descriptor would discard a wake the reactor has not yet polled.
    _wake: Arc<Waker>,
    /// The one active transfer and absolute deadline; no observer owns transport memory.
    pending: Option<(u64, Instant)>,
    /// Previous operation identity; write IDs are never reused in this session.
    last_id: Option<u64>,
    /// One worker command or report is outstanding.
    busy: bool,
    /// Coalesces edges received while a worker turn is in flight.
    dirty: bool,
    /// Cleanup publication is consumed once, even after a worker disconnect.
    cleanup_observed: bool,
}

impl Adapter {
    /// Attaches during blocking setup. The caller supplies a device token, its existing
    /// shared Waker and a nonempty worker budget; this method never creates a Poll.
    pub(in super::super) fn attach<B: Backend>(
        backend: B,
        registry: &Registry,
        token: Token,
        wake: Arc<Waker>,
        budget: NonZeroUsize,
    ) -> Result<Self, BusError> {
        let registry = registry.try_clone()?;
        let mut source = backend.readiness()?;
        // The idle worker retains the backend even if serving later panics. Registration
        // failure retires it without ever dropping USB ownership on the reactor.
        let worker = Worker::spawn(backend, Arc::clone(&wake), budget)?;
        if let Err(error) = registry.register(&mut *source, token, Interest::WRITABLE) {
            worker.retire(source);
            return Err(error.into());
        }
        Ok(Self {
            registry,
            source: Some(source),
            token,
            stopped: false,
            retirement_error: None,
            worker,
            _wake: wake,
            pending: None,
            last_id: None,
            busy: false,
            dirty: false,
            cleanup_observed: false,
        })
    }

    /// Admits one real machine transfer; the worker rechecks expiry before submission. A
    /// command needs a fresh, increasing identity; a reply transfer continues the last one.
    pub(in super::super) fn submit(
        &mut self,
        submission: &Submission,
        now: Instant,
    ) -> Result<(), BusError> {
        if self.stopped {
            return Err(BusError::Disconnected);
        }
        if self.pending.is_some() || self.busy {
            return Err(BusError::Unsupported("native USB action already pending"));
        }
        let Submission {
            id,
            transfer,
            timeout,
        } = *submission;
        match transfer {
            Transfer::Write(_) if self.last_id.is_some_and(|previous| id <= previous) => {
                return Err(BusError::Unsupported(
                    "native USB operation identity reused",
                ));
            }
            Transfer::Readback | Transfer::Notification if self.last_id != Some(id) => {
                return Err(BusError::Unsupported("native USB reply has no command"));
            }
            _ => {}
        }
        let expires = now
            .checked_add(timeout)
            .ok_or(BusError::Unsupported("native USB deadline overflow"))?;
        if timeout.is_zero() {
            return Err(BusError::Timeout);
        }
        self.worker.send(Work::Submit(Request {
            id,
            expires,
            transfer,
        }))?;
        self.last_id = Some(id);
        self.pending = Some((id, expires));
        self.busy = true;
        Ok(())
    }

    /// Coalesces a matching device readiness hint; stale/unrelated tokens do no work.
    pub(in super::super) fn on_ready(&mut self, token: Token) -> Result<(), BusError> {
        if !self.stopped && token == self.token && self.pending.is_some() {
            self.dirty = true;
            self.schedule()?;
        }
        Ok(())
    }

    /// Schedules exactly one bounded turn, preserving dirtiness while it is outstanding.
    fn schedule(&mut self) -> Result<(), BusError> {
        let Some((id, _)) = self.pending else {
            return Ok(());
        };
        if !self.stopped && self.dirty && !self.busy {
            self.worker.send(Work::Reap { id })?;
            self.busy = true;
            self.dirty = false;
        }
        Ok(())
    }

    /// Observes at most one worker report or deadline; never reaps, waits or cancels USB.
    pub(in super::super) fn try_event(
        &mut self,
        now: Instant,
    ) -> Result<Option<Completion>, BusError> {
        let Some((id, expires)) = self.pending else {
            return Ok(None);
        };
        // A report published before the deadline wins over a timer observed with it.
        let report = match self.worker.report() {
            Ok(Some(report)) => report,
            Ok(None) if now >= expires => return Ok(Some(self.fail(id, BusError::Timeout))),
            Ok(None) => return Ok(None),
            Err(error) => return Ok(Some(self.fail(id, error))),
        };
        if report.id() != id {
            return Ok(Some(self.fail(id, BusError::Disconnected)));
        }
        self.busy = false;
        match report {
            Report::Submitted { .. } => {
                self.dirty = true;
            }
            Report::Failed { error, .. }
            | Report::Turn {
                result: Err(error), ..
            } => return Ok(Some(self.fail(id, error))),
            Report::Turn {
                result: Ok(turn), ..
            } => {
                self.dirty |= turn.exhausted;
                if let Some(completion) = turn.completion {
                    if completion.id != id {
                        return Ok(Some(self.fail(id, BusError::Disconnected)));
                    }
                    if completion.result.is_err() {
                        self.retire_after_completion();
                    }
                    self.pending = None;
                    return Ok(Some(completion));
                }
            }
        }
        if now >= expires {
            return Ok(Some(self.fail(id, BusError::Timeout)));
        }
        self.schedule()?;
        Ok(None)
    }

    /// Publishes one terminal failure, retires registration and leaves all buffers owned.
    fn fail(&mut self, id: u64, error: BusError) -> Completion {
        self.pending = None;
        self.retire_after_completion();
        Completion {
            id,
            result: Err(error),
        }
    }

    /// Keeps terminal delivery independent of deregistration errors and cleanup.
    pub(in super::super) fn retire_after_completion(&mut self) {
        if let Err(error) = self.close() {
            self.retirement_error = Some(error);
        }
    }

    /// Takes an automatic retirement error once. `NotFound` has already retired the worker;
    /// any other error leaves the registration for a `close` retry.
    pub(in super::super) fn take_retirement_error(&mut self) -> Option<io::Error> {
        self.retirement_error.take()
    }

    /// Returns the pending timer to the caller's Poll policy; no internal timer thread.
    pub(in super::super) fn deadline(&self) -> Option<Instant> {
        self.pending.map(|(_, expires)| expires)
    }

    /// Deregisters, then retires the worker, which drops the source before cleaning up;
    /// requests cleanup but does not await it. `NotFound` means the entry is already gone, so
    /// it retires too and is reported. Any other deregistration error keeps the source
    /// registered for a retry, and the token must not be reused until one succeeds.
    pub(in super::super) fn close(&mut self) -> io::Result<()> {
        self.stopped = true;
        self.pending = None;
        self.dirty = false;
        let Some(source) = self.source.as_mut() else {
            return Ok(());
        };
        let deregistered = self.registry.deregister(&mut **source);
        if let Err(error) = &deregistered {
            if error.kind() != io::ErrorKind::NotFound {
                return deregistered;
            }
        }
        self.retire();
        self.retirement_error = None;
        deregistered
    }

    /// Hands the source and every USB owner to the worker for cleanup, never dropping
    /// them here: dropping a readiness can wait for an in-flight worker turn.
    fn retire(&mut self) {
        if let Some(source) = self.source.take() {
            self.worker.retire(source);
        }
    }

    /// Observes final cancellation and queue/claim Drop independently of result delivery.
    pub(in super::super) fn try_cleanup(&mut self) -> Option<Result<(), BusError>> {
        if self.source.is_some() || self.cleanup_observed {
            return None;
        }
        let result = self.worker.cleanup()?;
        self.cleanup_observed = true;
        Some(result)
    }
}

/// Drop requests retirement/cleanup, never waits for a syscall, and never joins a worker.
/// It retires even when deregistration fails: nothing could retry it, and the source's own
/// Drop deregisters again on the worker.
impl Drop for Adapter {
    fn drop(&mut self) {
        let _ = self.close();
        self.retire();
    }
}
