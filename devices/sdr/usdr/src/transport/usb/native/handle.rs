//! The public reactor-driven handle over the native adapter: one generic executor runs the
//! TMP114 read and portable sequences (gateware identity, reference-clock switch).

use std::fmt;
use std::io;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mio::{Registry, Token, Waker};

use super::super::i2c::I2cTransaction;
use super::super::machine::{Machine, Output, Step};
use super::super::protocol::I2cCache;
use super::adapter::Adapter;
use super::backend::Backend;
use crate::board::Board;
use crate::chips::si5332::Si5332;
use crate::chips::tmp114::{TemperatureRead, Tmp114};
use crate::error::Error;
use crate::fpga::IdentityRead;
use crate::identity::Identity;
use crate::lowlevel::BusError;
use crate::portable::sequence::{RegisterAccess, Sequence, SequenceAction};
use crate::portable::si5332::ClockSequence;
use crate::portable::si5332::sequence::SWITCH_WRITES;
use crate::portable::usb::register::RegisterTransaction;
use crate::reference::ReferenceClock;

/// Completions the worker files per turn; the handle keeps one request in flight.
const BUDGET: NonZeroUsize = NonZeroUsize::MIN.saturating_add(7);

/// Control-plane state carried over from the blocking bus, so the native session continues
/// its notification sequence baseline, I²C address table and operation identities.
pub(in super::super) struct Session {
    /// The simple I²C core's address table.
    pub i2c: I2cCache,
    /// Expected next global notification number.
    pub next_sequence: Option<u16>,
    /// Never-reused identity for the next native operation, I²C or register/SPI.
    pub next_id: u64,
    /// Revision 3, read from the hardware ID at hand-over: decides which Si5332 input
    /// [`ReferenceClock::Board`] selects.
    pub rev3: bool,
}

/// Something a [`NativeUsb`] finished.
#[derive(Debug)]
#[non_exhaustive]
pub enum NativeEvent {
    /// A temperature read finished, in °C. A failure also closes the handle.
    Temperature(Result<f32, Error>),
    /// A gateware identity read finished. A failure also closes the handle.
    Identity(Result<Identity, Error>),
    /// A reference-clock switch finished. A transport or protocol failure also closes the
    /// handle; [`Error::ClockInputMissing`] does not.
    ReferenceClock(Result<(), Error>),
}

/// The operation in flight: what consumes the executor's results.
enum Operation {
    /// A TMP114 read: one two-byte I²C transaction, then decoding.
    Temperature(TemperatureRead),
    /// A portable sequence, one action at a time.
    Sequence(SequenceOperation),
}

/// The sequences the handle runs, and how each reports its end.
enum SequenceOperation {
    /// The gateware build and hardware ID reads.
    Identity(IdentityRead),
    /// The Si5332 reference switch: state waits, polls and writes.
    ReferenceClock(ClockSequence<SWITCH_WRITES>),
}

impl SequenceOperation {
    /// The sequence's next action, as [`Sequence::next`].
    fn next(&self) -> Option<SequenceAction> {
        match self {
            Self::Identity(identity) => identity.next(),
            Self::ReferenceClock(switch) => switch.next(),
        }
    }

    /// Reports an action's outcome, as [`Sequence::complete`], with its register context.
    fn complete(&mut self, result: Result<u32, BusError>) -> Result<(), Error> {
        match self {
            Self::Identity(identity) => identity.complete(result),
            Self::ReferenceClock(switch) => switch.complete(result),
        }
        .map_err(Error::from)
    }

    /// Reports `error` as the failure of the next register access, with that access's chip
    /// and register, passing over any wait before it unperformed. Ends the sequence.
    fn fail_at_access(&mut self, error: BusError) -> Error {
        while let Some(SequenceAction::Sleep(_)) = self.next() {
            if self.complete(Ok(0)).is_err() {
                break;
            }
        }
        match self.complete(Err(error)) {
            Err(failure) => failure,
            Ok(()) => Error::Open(BusError::Unsupported(
                "a native sequence ended without reporting its failed access",
            )),
        }
    }

    /// The event for a run that ended with `result`.
    fn finish(self, result: Result<(), Error>) -> NativeEvent {
        match self {
            Self::Identity(identity) => NativeEvent::Identity(result.map(|()| {
                identity
                    .identity()
                    .expect("invariant: a finished identity sequence read both words")
            })),
            Self::ReferenceClock(_) => NativeEvent::ReferenceClock(result),
        }
    }
}

/// What the pending operation waits for.
#[expect(
    clippy::large_enum_variant,
    reason = "one per handle; boxing the machine would allocate per action"
)]
enum Wait {
    /// The worker's next result for this machine's transfer.
    Transfer(Machine),
    /// A sequence's wait, over at this instant; no I/O.
    Sleep(Instant),
}

/// The one operation in flight and its current wait.
struct Pending {
    /// Consumes results and names the next action.
    operation: Operation,
    /// The current action.
    wait: Wait,
}

/// A uSDR's USB control plane driven by a caller-owned Mio reactor (feature `mio`), made by
/// [`UsbBus::into_native`](super::super::UsbBus::into_native).
///
/// Unstable pilot: it reads the TMP114 temperature and the gateware identity, and switches
/// the reference clock. It powers nothing, so it applies no thermal policy. The reactor
/// thread only registers, schedules and observes; a worker thread owns the USB queues and
/// performs every USB call, including cleanup. Waits inside an operation are deadlines the
/// caller's reactor honours, never sleeps.
///
/// Drive it from your event loop: pass every event for its token to
/// [`on_ready`](Self::on_ready), then call [`poll`](Self::poll) after any event for its
/// token or the shared [`Waker`], and once [`deadline`](Self::deadline) passes. Pass a
/// `now` that never decreases across calls; a step backwards fails the operation. One
/// operation is in flight at a time. Any failure closes the handle, except refusing a
/// second operation, a rejected argument, or a failure a sequence finds in well-formed
/// replies (such as [`Error::ClockInputMissing`]); open the device again to continue.
/// Unqualified on hardware.
pub struct NativeUsb {
    /// Registration, deadlines and the worker's mailboxes.
    adapter: Adapter,
    /// Carried over from the blocking bus.
    session: Session,
    /// Origin of the machines' caller-supplied time.
    epoch: Instant,
    /// The operation in flight.
    pending: Option<Pending>,
    /// A worker handoff failure noticed outside `poll`, reported by the next `poll`.
    broken: Option<BusError>,
}

impl fmt::Debug for NativeUsb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeUsb")
            .field("pending", &self.pending.is_some())
            .finish_non_exhaustive()
    }
}

impl NativeUsb {
    /// Attaches `backend` to `registry` under `token`, continuing `session`.
    pub(in super::super) fn attach<B: Backend>(
        backend: B,
        session: Session,
        registry: &Registry,
        token: Token,
        waker: Arc<Waker>,
    ) -> Result<Self, BusError> {
        Ok(Self {
            adapter: Adapter::attach(backend, registry, token, waker, BUDGET)?,
            session,
            epoch: Instant::now(),
            pending: None,
            broken: None,
        })
    }

    /// Starts reading the TMP114 temperature; [`poll`](Self::poll) reports the result.
    ///
    /// # Errors
    ///
    /// [`Error::Busy`] if an operation is already pending; [`Error::Bus`] if the handle is
    /// closed or the command cannot be handed to the worker, which also closes the handle.
    pub fn start_temperature(&mut self, now: Instant) -> Result<(), Error> {
        if self.pending.is_some() {
            return Err(Error::Busy);
        }
        let (mut read, request) = TemperatureRead::begin(Tmp114::usdr());
        let access = RegisterAccess::Read {
            dev: request.dev,
            reg: request.register[0],
        };
        match self.i2c(access, 2, now) {
            Ok(wait) => {
                self.pending = Some(Pending {
                    operation: Operation::Temperature(read),
                    wait,
                });
                Ok(())
            }
            Err(error) => {
                self.adapter.retire_after_completion();
                Err(read.fail(error))
            }
        }
    }

    /// Starts reading the gateware build and hardware ID, as
    /// [`Device::identity`](crate::Device::identity) does; [`poll`](Self::poll) reports it.
    ///
    /// # Errors
    ///
    /// [`Error::Busy`] if an operation is already pending; [`Error::Bus`] if the handle is
    /// closed or the first read cannot be handed to the worker, which also closes the handle.
    pub fn start_identity(&mut self, now: Instant) -> Result<(), Error> {
        self.start_sequence(SequenceOperation::Identity(IdentityRead::default()), now)
    }

    /// Starts moving the clocks' reference to `reference`, as
    /// [`Device::set_reference_clock`](crate::Device::set_reference_clock) does, but without
    /// any board state: like libusdr, the sample clocks and synthesizers keep their
    /// settings. [`poll`](Self::poll) reports the result.
    ///
    /// # Errors
    ///
    /// - [`Error::UnsupportedReferenceClock`] for an external reference the clock plans
    ///   cannot use, before any I/O; the handle stays open.
    /// - [`Error::Busy`] if an operation is already pending.
    /// - [`Error::Bus`] if the handle is closed.
    pub fn start_reference_clock(
        &mut self,
        reference: ReferenceClock,
        now: Instant,
    ) -> Result<(), Error> {
        reference.check()?;
        let input = Board::si5332_reference(reference, self.session.rev3);
        let switch = ClockSequence::reference_switch(Si5332::usdr().dev(), input);
        self.start_sequence(SequenceOperation::ReferenceClock(switch), now)
    }

    /// Starts a sequence operation's first action, refusing it while another is pending.
    fn start_sequence(
        &mut self,
        mut operation: SequenceOperation,
        now: Instant,
    ) -> Result<(), Error> {
        if self.pending.is_some() {
            return Err(Error::Busy);
        }
        // An empty sequence finishes at the next poll: its passed wait completes nothing,
        // which a sequence ignores.
        let wait = match operation.next().map(|action| self.begin(action, now)) {
            None => Wait::Sleep(now),
            Some(Ok(wait)) => wait,
            Some(Err(error)) => {
                self.adapter.retire_after_completion();
                return Err(operation.fail_at_access(error));
            }
        };
        self.pending = Some(Pending {
            operation: Operation::Sequence(operation),
            wait,
        });
        Ok(())
    }

    /// Starts `action`: a machine whose command goes to the worker now, or a wait.
    fn begin(&mut self, action: SequenceAction, now: Instant) -> Result<Wait, BusError> {
        match action {
            SequenceAction::Sleep(duration) => now
                .checked_add(duration)
                .map(Wait::Sleep)
                .ok_or(BusError::Unsupported("native wait overflows the clock")),
            SequenceAction::Access(access) => {
                let read = match access {
                    RegisterAccess::Read { .. } => 1,
                    RegisterAccess::Write { .. } => 0,
                };
                self.i2c(access, read, now)
            }
            SequenceAction::Fpga(access) => {
                let id = self.take_id()?;
                let transaction = RegisterTransaction::access(id, access, self.elapsed(now))?;
                self.launch(Machine::Register(transaction), now)
            }
        }
    }

    /// Starts an I²C register access reading `read` bytes back.
    fn i2c(&mut self, access: RegisterAccess, read: usize, now: Instant) -> Result<Wait, BusError> {
        let id = self.take_id()?;
        let (dev, bytes) = match access {
            RegisterAccess::Read { dev, reg } => (dev, [reg, 0]),
            RegisterAccess::Write { dev, reg, value } => (dev, [reg, value]),
        };
        let write = match access {
            RegisterAccess::Read { .. } => &bytes[..1],
            RegisterAccess::Write { .. } => &bytes[..],
        };
        let elapsed = self.elapsed(now);
        let transaction =
            I2cTransaction::new(id, dev, write, read, &mut self.session.i2c, elapsed)?;
        self.launch(Machine::I2c(transaction), now)
    }

    /// Starts `machine` and hands its command to the worker.
    fn launch(&mut self, mut machine: Machine, now: Instant) -> Result<Wait, BusError> {
        let Step::Submit(submission) = machine.start(self.elapsed(now))? else {
            return Err(BusError::Unsupported(
                "native USB machine finished without a transfer",
            ));
        };
        self.adapter.submit(&submission, now)?;
        Ok(Wait::Transfer(machine))
    }

    /// The next never-reused operation identity.
    fn take_id(&mut self) -> Result<u64, BusError> {
        let id = self.session.next_id;
        self.session.next_id = id.checked_add(1).ok_or(BusError::Unsupported(
            "USB operation identity exhausted; reopen it",
        ))?;
        Ok(id)
    }

    /// Forwards one Mio event's token; events for other tokens are ignored.
    pub fn on_ready(&mut self, token: Token) {
        if let Err(error) = self.adapter.on_ready(token) {
            self.broken.get_or_insert(error);
        }
    }

    /// Observes the worker and deadlines once and starts the operation's next action; never
    /// waits or performs USB I/O.
    #[must_use]
    pub fn poll(&mut self, now: Instant) -> Option<NativeEvent> {
        let mut pending = self.pending.take()?;
        let Some(outcome) = self.observe(&mut pending.wait, now) else {
            self.pending = Some(pending);
            return None;
        };
        let mut operation = match pending.operation {
            Operation::Temperature(read) => return Some(self.conclude(read, outcome)),
            Operation::Sequence(operation) => operation,
        };
        match self.advance(&mut operation, outcome, now) {
            Ok(Some(wait)) => {
                self.pending = Some(Pending {
                    operation: Operation::Sequence(operation),
                    wait,
                });
                None
            }
            Ok(None) => Some(operation.finish(Ok(()))),
            Err(error) => Some(operation.finish(Err(error))),
        }
    }

    /// The current action's outcome, once there is one: the machine's output, or `None` for
    /// a wait that has passed. A worker handoff failure is the outcome of the next transfer.
    fn observe(
        &mut self,
        wait: &mut Wait,
        now: Instant,
    ) -> Option<Result<Option<Output>, BusError>> {
        let machine = match wait {
            Wait::Sleep(until) => return (now >= *until).then_some(Ok(None)),
            Wait::Transfer(machine) => machine,
        };
        if let Some(error) = self.broken.take() {
            return Some(Err(error));
        }
        let completion = match self.adapter.try_event(now) {
            Ok(Some(completion)) => completion,
            Ok(None) => return None,
            Err(error) => return Some(Err(error)),
        };
        let elapsed = self.elapsed(now);
        let step = completion.apply(machine, elapsed, &mut self.session.next_sequence);
        match step {
            Ok(Step::Done(output)) => Some(Ok(Some(output))),
            Ok(Step::Submit(submission)) => self.adapter.submit(&submission, now).err().map(Err),
            Err(error) => Some(Err(error)),
        }
    }

    /// Reports the current action's outcome to the sequence and starts its next action;
    /// `None` once the sequence has finished. A transfer failure ends the session, since a
    /// late reply could still arrive; a failure the sequence finds in good data does not.
    fn advance(
        &mut self,
        operation: &mut SequenceOperation,
        outcome: Result<Option<Output>, BusError>,
        now: Instant,
    ) -> Result<Option<Wait>, Error> {
        if outcome.is_err() {
            self.adapter.retire_after_completion();
        }
        operation.complete(outcome.map(|output| output.map_or(0, Output::first)))?;
        let Some(action) = operation.next() else {
            return Ok(None);
        };
        self.begin(action, now).map(Some).map_err(|error| {
            self.adapter.retire_after_completion();
            operation.fail_at_access(error)
        })
    }

    /// Resolves the temperature read; any failure, malformed data included, ends the session.
    fn conclude(
        &mut self,
        mut read: TemperatureRead,
        outcome: Result<Option<Output>, BusError>,
    ) -> NativeEvent {
        let result = match outcome {
            Ok(Some(Output::I2c { bytes, len })) => read.resolve(Ok(&bytes[..len])),
            Ok(_) => read.resolve(Err(BusError::Unsupported(
                "native temperature read completed without I2C bytes",
            ))),
            Err(error) => read.resolve(Err(error)),
        };
        if result.is_err() {
            self.adapter.retire_after_completion();
        }
        NativeEvent::Temperature(result)
    }

    /// The machines' time for `now`.
    fn elapsed(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.epoch)
    }

    /// When [`poll`](Self::poll) must run even without an event.
    #[must_use]
    pub fn deadline(&self) -> Option<Instant> {
        match self.pending {
            Some(Pending {
                wait: Wait::Sleep(until),
                ..
            }) => Some(until),
            _ => self.adapter.deadline(),
        }
    }

    /// Deregisters and lets the worker cancel, release the device and report through
    /// [`try_cleanup`](Self::try_cleanup); never waits.
    ///
    /// # Errors
    ///
    /// The deregistration failure. `NotFound` has still retired the worker; after any other
    /// error the token stays registered and `close` may be retried.
    pub fn close(&mut self) -> io::Result<()> {
        self.pending = None;
        self.adapter.close()
    }

    /// The worker's cleanup result, once, after [`close`](Self::close) or a failure.
    pub fn try_cleanup(&mut self) -> Option<Result<(), BusError>> {
        self.adapter.try_cleanup()
    }

    /// A deregistration error from a failure's automatic close, once.
    pub fn take_retirement_error(&mut self) -> Option<io::Error> {
        self.adapter.take_retirement_error()
    }
}
