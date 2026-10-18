//! Registration, deadlines and coalesced worker scheduling; no USB syscalls here.

use std::collections::VecDeque;
use std::io;
use std::mem;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Instant;

use mio::event::Source;
use mio::{Interest, Registry, Token, Waker};

use super::super::machine::{Submission, Transfer};
use super::backend::{Backend, Completion, Request};
use super::handle::RxBlock;
use super::worker::{Harvest, Report, StreamCommand, Work, Worker};
use crate::lowlevel::BusError;

/// RX blocks copied off the device and not yet taken by the handle: one ring's worth.
/// Turns take no more, so a caller that stops polling stops the copying, and the device's
/// ring then runs dry and the gateware counts what it drops.
pub(in super::super) const RX_BACKLOG: usize = 8;

/// The answer to an operation's stream work.
pub(in super::super) enum StreamOutcome {
    /// A TX acquisition attempt: whether a free transfer is now held for the next commit.
    Acquired(Result<bool, BusError>),
    /// A stream command's result.
    Command(Result<(), BusError>),
}

/// Native executor registered in an externally owned Poll: one control request, one stream
/// command and one reap turn at a time.
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent admission, mailbox, edge, interest and cleanup-publication facts"
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
    /// The one active control transfer and absolute deadline; no observer owns transport
    /// memory.
    pending: Option<(u64, Instant)>,
    /// Previous operation identity; write IDs are never reused in this session.
    last_id: Option<u64>,
    /// An operation's command (a control submission or a stream command) awaits its report.
    commanding: bool,
    /// When the outstanding stream command's report is due: past it, the worker is taken
    /// for stuck in the kernel and the session ends.
    command_due: Option<Instant>,
    /// A reap turn awaits its report.
    turning: bool,
    /// Coalesces edges received while a worker turn is in flight or nothing wanted a turn.
    dirty: bool,
    /// Turns take RX blocks, up to the room left in `blocks`.
    rx_wanted: bool,
    /// A turn ran while RX was wanted but `blocks` was full: it may have filed RX
    /// completions that no later edge announces, so freeing room calls for a turn.
    rx_starved: bool,
    /// Turns try for a free TX transfer.
    tx_wanted: bool,
    /// Counts the sends that wanted a free TX transfer; a turn carries the current one, so
    /// an older turn's "every transfer in flight" is not taken for a newer send's attempt.
    tx_generation: u64,
    /// A control completion drained from the worker, for the next `try_event`.
    completed: Option<Completion>,
    /// RX blocks drained from the worker, oldest first; at most [`RX_BACKLOG`].
    blocks: VecDeque<Result<RxBlock, BusError>>,
    /// Acquisitions and stream-command results drained from the worker, oldest first.
    outcomes: VecDeque<StreamOutcome>,
    /// Why the adapter stopped by failure, for the streams; the control request, if one was
    /// pending, carries the original.
    fault: Option<BusError>,
    /// The failure that stopped the adapter came with no control request to carry it.
    unclaimed: bool,
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
            commanding: false,
            command_due: None,
            turning: false,
            dirty: false,
            rx_wanted: false,
            rx_starved: false,
            tx_wanted: false,
            tx_generation: 0,
            completed: None,
            blocks: VecDeque::new(),
            outcomes: VecDeque::new(),
            fault: None,
            unclaimed: false,
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
        self.admissible()?;
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
        self.commanding = true;
        Ok(())
    }

    /// Hands one stream command to the worker at `now`; its result arrives as
    /// [`StreamOutcome::Command`], or, once [`StreamCommand::deadline`] has passed without
    /// one, as a timeout that ends the session (see [`Self::try_outcome`]).
    pub(in super::super) fn command(
        &mut self,
        command: StreamCommand,
        now: Instant,
    ) -> Result<(), BusError> {
        self.admissible()?;
        let due = now
            .checked_add(command.deadline())
            .ok_or(BusError::Unsupported("native USB deadline overflow"))?;
        self.worker.send(Work::Stream(command))?;
        self.commanding = true;
        self.command_due = Some(due);
        Ok(())
    }

    /// Refuses a new operation command while stopped or while another is outstanding.
    fn admissible(&self) -> Result<(), BusError> {
        if self.stopped {
            return Err(BusError::Disconnected);
        }
        if self.pending.is_some() || self.commanding {
            return Err(BusError::Unsupported("native USB action already pending"));
        }
        Ok(())
    }

    /// Starts or stops taking RX blocks. Starting covers an edge that came before it;
    /// stopping drops the blocks not yet taken.
    pub(in super::super) fn want_rx(&mut self, wanted: bool) {
        self.rx_wanted = wanted;
        self.rx_starved = false;
        if wanted {
            self.dirty = true;
            self.schedule();
        } else {
            self.blocks.clear();
        }
    }

    /// Starts or stops trying for a free TX transfer on each turn, for a new send when
    /// starting. Starting covers an edge that came before it.
    pub(in super::super) fn want_tx(&mut self, wanted: bool) {
        self.tx_wanted = wanted;
        if wanted {
            self.tx_generation = self.tx_generation.wrapping_add(1);
            self.dirty = true;
            self.schedule();
        }
    }

    /// Coalesces a matching device readiness hint; stale/unrelated tokens do no work.
    pub(in super::super) fn on_ready(&mut self, token: Token) {
        if !self.stopped && token == self.token {
            self.dirty = true;
            self.schedule();
        }
    }

    /// Schedules exactly one bounded turn after an edge or an exhausted budget, preserving
    /// dirtiness while a turn is outstanding. An idle session takes its turn too: an edge
    /// with nothing in flight is the device's hang-up or error, which only a reap reports.
    fn schedule(&mut self) {
        if self.stopped || !self.dirty || self.turning {
            return;
        }
        let rx = if self.rx_wanted {
            RX_BACKLOG - self.blocks.len()
        } else {
            0
        };
        let acquire = self.tx_wanted.then_some(self.tx_generation);
        match self.worker.send(Work::Turn { rx, acquire }) {
            Ok(()) => {
                self.turning = true;
                self.dirty = false;
                // Its reap can file RX completions it has no room to take.
                self.rx_starved |= self.rx_wanted && rx == 0;
            }
            Err(error) => self.collapse(error),
        }
    }

    /// Observes the worker's reports and the control deadline; returns the control
    /// request's completion once. Never reaps, waits or cancels USB.
    pub(in super::super) fn try_event(&mut self, now: Instant) -> Option<Completion> {
        self.pump();
        // A report published before the deadline wins over a timer observed with it.
        if self.completed.is_none() {
            if let Some((id, expires)) = self.pending {
                if now >= expires {
                    self.completed = Some(self.fail(id, BusError::Timeout));
                }
            }
        }
        self.schedule();
        self.completed.take()
    }

    /// The oldest RX block or RX failure the worker copied, freeing room for another.
    pub(in super::super) fn try_block(&mut self) -> Option<Result<RxBlock, BusError>> {
        self.pump();
        let block = self.blocks.pop_front();
        if block.is_some() && mem::take(&mut self.rx_starved) {
            self.dirty = true;
        }
        self.schedule();
        block
    }

    /// The oldest acquisition or stream-command result. A stream command the worker has not
    /// answered by its deadline at `now` fails with [`BusError::Timeout`] and ends the
    /// session: the worker is stuck in the kernel, and whatever it answers later is never
    /// read. Its cleanup still runs on the worker once the call returns.
    pub(in super::super) fn try_outcome(&mut self, now: Instant) -> Option<StreamOutcome> {
        self.pump();
        if self.command_due.is_some_and(|due| now >= due) {
            self.command_due = None;
            self.commanding = false;
            self.outcomes
                .push_back(StreamOutcome::Command(Err(BusError::Timeout)));
            // Open streams outlive the command: they learn the session ended.
            self.fault.get_or_insert(BusError::Timeout);
            self.retire_after_completion();
        }
        let outcome = self.outcomes.pop_front();
        self.schedule();
        outcome
    }

    /// Drains every report the worker has published, routing each. [`NativeUsb::poll`]
    /// calls it first; each `try_*` calls it again before it looks.
    ///
    /// [`NativeUsb::poll`]: super::NativeUsb::poll
    pub(in super::super) fn pump(&mut self) {
        while !self.stopped {
            match self.worker.report() {
                Ok(Some(report)) => self.route(report),
                Ok(None) => return,
                Err(error) => self.collapse(error),
            }
        }
    }

    /// Files one report under what it answers.
    fn route(&mut self, report: Report) {
        match report {
            Report::Submitted { id } => {
                self.commanding = false;
                if self.pending.is_some_and(|(pending, _)| pending == id) {
                    self.dirty = true;
                } else {
                    self.collapse(BusError::Disconnected);
                }
            }
            Report::Failed { id, error } => {
                self.commanding = false;
                let error = match self.pending {
                    Some((pending, _)) if pending == id => error,
                    _ => BusError::Disconnected,
                };
                self.collapse(error);
            }
            Report::Stream(result) => {
                self.commanding = false;
                self.command_due = None;
                self.outcomes.push_back(StreamOutcome::Command(result));
            }
            Report::Turn(Err(error)) => {
                self.turning = false;
                self.collapse(error);
            }
            Report::Turn(Ok(harvest)) => {
                self.turning = false;
                self.harvest(harvest);
            }
        }
    }

    /// Files a turn's stream work and control completion.
    fn harvest(&mut self, harvest: Harvest) {
        let Harvest {
            completion,
            rx,
            acquired,
            exhausted,
        } = harvest;
        self.dirty |= exhausted;
        // A turn taken before RX stopped being wanted can still bring blocks: drop them.
        if self.rx_wanted {
            self.blocks.extend(rx);
        }
        if let Some((generation, acquired)) = acquired {
            // An older send's "every transfer in flight" is no attempt for the current one;
            // a held transfer or a failure still is.
            let stale = generation != self.tx_generation && matches!(acquired, Ok(false));
            if !stale {
                self.outcomes.push_back(StreamOutcome::Acquired(acquired));
            }
        }
        let Some(completion) = completion else {
            return;
        };
        if self.pending.is_none_or(|(id, _)| id != completion.id) {
            self.collapse(BusError::Disconnected);
            return;
        }
        if completion.result.is_err() {
            self.retire_after_completion();
        }
        self.pending = None;
        self.completed = Some(completion);
    }

    /// Ends the session on a failure no single operation owns: a pending control request
    /// gets `error`, and the streams it (or a copy) through [`take_fault`](Self::take_fault).
    fn collapse(&mut self, error: BusError) {
        if let Some((id, _)) = self.pending {
            // Open streams outlive the control request: they get a copy.
            self.fault.get_or_insert_with(|| duplicate(&error));
            self.completed = Some(self.fail(id, error));
        } else {
            self.fault.get_or_insert(error);
            self.unclaimed = true;
            self.retire_after_completion();
        }
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

    /// Whether the session has ended, by failure or [`close`](Self::close).
    pub(in super::super) fn is_stopped(&self) -> bool {
        self.stopped
    }

    /// The failure that ended the session when no control request carried it, once.
    pub(in super::super) fn take_fault(&mut self) -> Option<BusError> {
        self.fault.take()
    }

    /// The failure that ended the session with no control request to carry it (a worker
    /// panic or lost report, a failed turn while idle), once, unless a stream took it.
    /// Only what an earlier [`pump`](Self::pump) routed: a report that arrives after the
    /// stream looked waits for the next pump, so an open stream still reports it.
    pub(in super::super) fn take_unclaimed(&mut self) -> Option<BusError> {
        if !mem::take(&mut self.unclaimed) {
            return None;
        }
        self.fault.take()
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

    /// Returns the pending timer to the caller's Poll policy (the control request's, or the
    /// stream command's); no internal timer thread.
    pub(in super::super) fn deadline(&self) -> Option<Instant> {
        let control = self.pending.map(|(_, expires)| expires);
        match (control, self.command_due) {
            (Some(control), Some(command)) => Some(control.min(command)),
            (control, command) => control.or(command),
        }
    }

    /// Deregisters, then retires the worker, which drops the source before cleaning up
    /// (streams included); requests cleanup but does not await it. `NotFound` means the
    /// entry is already gone, so it retires too and is reported. Any other deregistration
    /// error keeps the source registered for a retry, and the token must not be reused
    /// until one succeeds.
    pub(in super::super) fn close(&mut self) -> io::Result<()> {
        self.stopped = true;
        self.pending = None;
        self.command_due = None;
        self.dirty = false;
        self.rx_wanted = false;
        self.tx_wanted = false;
        self.blocks.clear();
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

/// A copy of `error` for a second recipient: exact for the plain variants, kind and message
/// for an OS error, the message for any other.
fn duplicate(error: &BusError) -> BusError {
    match error {
        BusError::Timeout => BusError::Timeout,
        BusError::Disconnected => BusError::Disconnected,
        BusError::Unsupported(reason) => BusError::Unsupported(reason),
        BusError::Io(error) => BusError::Io(io::Error::new(error.kind(), error.to_string())),
        other => BusError::Io(io::Error::other(other.to_string())),
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
