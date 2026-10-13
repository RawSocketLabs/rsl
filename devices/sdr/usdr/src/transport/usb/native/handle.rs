//! The public reactor-driven handle over the native adapter: one generic executor runs the
//! TMP114 read and portable sequences (gateware identity, reference-clock switch, the
//! board's RF operations: `radio`), the RX and TX streams' operations at the transport
//! (`stream`), and the stream group's create, start and stop, which compose them
//! (`group`).

mod group;
mod radio;
mod stream;

use std::fmt;
use std::io;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mio::{Registry, Token, Waker};

use self::group::{GroupRun, NativeGroup};
use self::radio::RadioRun;
use self::stream::{Send, StreamOperation};
use super::super::i2c::I2cTransaction;
use super::super::machine::{Machine, Output, Step};
use super::super::protocol::I2cCache;
use super::super::tx::TxStatus;
use super::adapter::Adapter;
use super::backend::Backend;
use super::window::Windowed;
use crate::board::Board;
use crate::chips::si5332::Si5332;
use crate::chips::tmp114::{TemperatureRead, Tmp114};
use crate::error::Error;
use crate::fpga::{self, IdentityRead, IqImbalance};
use crate::identity::Identity;
use crate::lowlevel::BusError;
use crate::portable::radio::{Chains, PowerUpPlan, Radio};
use crate::portable::sequence::{RegisterAccess, Sequence, SequenceAction};
use crate::portable::si5332::ClockSequence;
use crate::portable::si5332::sequence::SWITCH_WRITES;
use crate::portable::stream::EngineSequence;
use crate::portable::usb::register::RegisterTransaction;
use crate::reference::ReferenceClock;
use crate::thermal::ThermalPolicy;

/// Completions the worker files per turn; the handle keeps one request in flight.
const BUDGET: NonZeroUsize = NonZeroUsize::MIN.saturating_add(7);

/// Control-plane state carried over from the blocking bus, so the native session continues
/// its notification sequence baseline, I²C address table, operation identities and TX
/// status throttle, and the board state a stream group needs.
pub(in super::super) struct Session {
    /// The simple I²C core's address table.
    pub i2c: I2cCache,
    /// Expected next global notification number.
    pub next_sequence: Option<u16>,
    /// Never-reused identity for the next native operation, I²C or register/SPI.
    pub next_id: u64,
    /// The board state carried on.
    pub board: BoardState,
    /// The TX core's throttled status words, per device as in libusdr.
    pub tx_status: TxStatus,
    /// The thermal policy the handle enforces: the handing-over device's, or the one its
    /// power-up was given; `None` before either.
    pub thermal: Option<ThermalPolicy>,
}

/// What a native handle carries on from the board it takes over: the facts the board's
/// identity established, the IQ corrections a stream group's create writes again, and the
/// powered board's RF state, which the handle's RF operations change.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BoardState {
    /// Revision 3: decides which Si5332 input [`ReferenceClock::Board`] selects.
    pub(crate) rev3: bool,
    /// The gateware's chains; on the extended TX core a stream group refuses TX before any
    /// I/O, as the blocking device does.
    pub(crate) chains: Chains,
    /// The powered board's RF state: a handed-over device's, or the handle's own after its
    /// power-up; `None` on an unpowered board.
    pub(crate) radio: Option<Radio>,
    /// The RX chain's IQ correction (libusdr's `rx_corr`), which an RX create writes
    /// again, as the blocking device's does: a handed-over device's, else a fresh board's.
    pub(crate) rx_iq_imbalance: IqImbalance,
    /// The TX chain's IQ correction (libusdr's `tx_corr`), which a TX create writes again;
    /// likewise.
    pub(crate) tx_iq_imbalance: IqImbalance,
}

/// Something a [`NativeUsb`] finished or received.
#[derive(Debug)]
#[non_exhaustive]
pub enum NativeEvent {
    /// A temperature read finished, in °C. A failure also closes the handle, except
    /// [`Error::Overheated`]: a powered board with a thermal policy (the handing-over
    /// device's, or the power-up's) at or above the policy's stop limit; stop streaming and
    /// power the board down.
    Temperature(Result<f32, Error>),
    /// A gateware identity read finished. A failure also closes the handle.
    Identity(Result<Identity, Error>),
    /// A reference-clock switch finished. A transport or protocol failure also closes the
    /// handle; [`Error::ClockInputMissing`] does not.
    ReferenceClock(Result<(), Error>),
    /// RX opened and primed its transfers, or failed to; after a failure, close RX before
    /// opening it again.
    RxOpened(Result<(), Error>),
    /// One RX block arrived, or RX failed ([`Error::Stream`]). A failed stream delivers
    /// nothing more until it is closed and opened again; the handle stays open, unless the
    /// failure was the session's own end (a transport failure that closed the handle),
    /// which an open stream reports here once.
    Received(Result<RxBlock, Error>),
    /// RX closed: every transfer was cancelled and came back, and the endpoint is free. On
    /// failure the endpoint stays held; close again.
    RxClosed(Result<(), Error>),
    /// TX opened, or failed to; after a failure, close TX before opening it again.
    TxOpened(Result<(), Error>),
    /// A send queued its block, reporting the TX core's four status words as libusdr
    /// reports them with it: read afresh on the device's first counted send and every 64th
    /// after, else the last reading. [`Error::Timeout`] if no transfer freed in time (the
    /// send is then not counted); [`Error::Stream`] if the stream refused or failed it. A
    /// failed status read closes the handle.
    Sent(Result<[u32; 4], Error>),
    /// TX closed: every transfer, sent or not, was cancelled and came back. On failure the
    /// endpoint stays held; close again.
    TxClosed(Result<(), Error>),
    /// The stream group's create finished: each stream's engine is configured and its
    /// transport open, or the first failure. As on the blocking device, a stream whose
    /// transport opened stays in the group after a later failure, until the group stops.
    /// A stream whose transport open itself failed is not in the group and stays held:
    /// close it with [`NativeUsb::start_rx_close`] or [`NativeUsb::start_tx_close`] before
    /// creating a group again.
    StreamsCreated(Result<(), Error>),
    /// The stream group started, or failed to.
    StreamsStarted(Result<(), Error>),
    /// The RX DMA engine was told the host is ready, or the write failed.
    RxReady(Result<(), Error>),
    /// The TX core's lanes were muted as asked, or the write failed; the group keeps the
    /// setting for its start either way.
    TxMuted(Result<(), Error>),
    /// The RX front end's bursts were throttled as asked, or the write failed.
    RxThrottled(Result<(), Error>),
    /// The board powered up, or failed to (refused as too hot before anything powered, or
    /// failed partway, with the RF section turned back off as libusdr's `fail:` path does).
    PoweredUp(Result<(), Error>),
    /// The board powered down: every write was attempted; the first failure, if any.
    PoweredDown(Result<(), Error>),
    /// A sample rate was set, or failed.
    SampleRateSet(Result<(), Error>),
    /// A chain was tuned, or failed.
    Tuned(Result<(), Error>),
    /// A filter bandwidth was set, or failed.
    BandwidthSet(Result<(), Error>),
    /// A gain was set, or failed.
    GainSet(Result<(), Error>),
    /// A path was selected, or failed.
    PathSet(Result<(), Error>),
    /// The stream group stopped: every step was attempted and the group is gone; the first
    /// failure, if any. A stream whose transport failed to close stays held; close it
    /// with [`NativeUsb::start_rx_close`] or [`NativeUsb::start_tx_close`]. A register
    /// transfer failure instead ends the session: the handle closes, the later steps fail
    /// at once, and the worker's cleanup cancels the streams (see
    /// [`NativeUsb::try_cleanup`]).
    StreamsStopped(Result<(), Error>),
}

/// One RX block a [`NativeUsb`] received, copied off its transfer, which was then
/// requeued.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RxBlock {
    /// The block's sample bytes in the stream's wire format, without the transport's
    /// 16-byte trailer.
    pub payload: Vec<u8>,
    /// The trailer's first two words, as libusdr's `usb_uram_recv_dma_wait` passes them on
    /// as its out-of-band data: in the first, bits 23:0 count the packets the gateware
    /// dropped before this one and the high 32 bits are the burst mask.
    pub oob: [u64; 2],
}

/// The operation in flight, with what it waits for.
enum Pending {
    /// A TMP114 read: one two-byte I²C transaction, then decoding.
    Temperature(TemperatureRead, Machine),
    /// A portable sequence, one action at a time.
    Sequence(SequenceOperation, Wait),
    /// A stream command, awaiting the worker's answer.
    Stream(StreamOperation),
    /// A TX send.
    Send(Send),
    /// A stream-group operation's steps.
    Group(GroupRun),
    /// A power-up's temperature check, then its plan.
    Thermal(TemperatureRead, Machine, PowerUpPlan),
}

/// The sequences the handle runs, and how each reports its end.
#[expect(
    clippy::large_enum_variant,
    reason = "one per handle; boxing the machine would allocate per operation"
)]
enum SequenceOperation {
    /// The gateware build and hardware ID reads.
    Identity(IdentityRead),
    /// The Si5332 reference switch: state waits, polls and writes; and the reference it
    /// switches to.
    ReferenceClock(ClockSequence<SWITCH_WRITES>, ReferenceClock),
    /// A board RF operation.
    Radio(RadioRun),
    /// A stream group's engine registers, through the index window. A group step.
    /// Also a stream create's DSP-port writes, which need no window.
    Engine(Windowed<EngineSequence>),
    /// A TX create's gateware read, then its minimum-build check. A group step.
    TxGate(IdentityRead),
}

/// Where a sequence is after an outcome.
#[expect(
    clippy::large_enum_variant,
    reason = "moved straight back into the pending operation; boxing would allocate per action"
)]
enum Progress {
    /// It waits for its current action.
    Running(SequenceOperation, Wait),
    /// It ended with this result.
    Finished(SequenceOperation, Result<(), Error>),
}

impl SequenceOperation {
    /// The sequence's next action, as [`Sequence::next`].
    fn next(&self) -> Option<SequenceAction> {
        match self {
            Self::Identity(identity) | Self::TxGate(identity) => identity.next(),
            Self::ReferenceClock(switch, _) => switch.next(),
            Self::Engine(engine) => engine.next(),
            Self::Radio(run) => run.next(),
        }
    }

    /// Reports an action's outcome, as [`Sequence::complete`], with its register context.
    fn complete(&mut self, result: Result<u32, BusError>) -> Result<(), Error> {
        match self {
            Self::Identity(identity) | Self::TxGate(identity) => identity.complete(result),
            Self::ReferenceClock(switch, _) => switch.complete(result),
            Self::Engine(engine) => engine.complete(result),
            Self::Radio(run) => return run.complete(result),
        }
        .map_err(Error::from)
    }

    /// A group step's result for a run that ended with `result`: a TX gate also refuses
    /// gateware older than libusdr's minimum, as the blocking TX create does.
    fn step_result(&self, result: Result<(), Error>) -> Result<(), Error> {
        let Self::TxGate(identity) = self else {
            return result;
        };
        result?;
        let build = identity
            .identity()
            .expect("invariant: a finished identity sequence read both words")
            .gateware_build;
        fpga::check_tx_gateware(build)
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
            Self::ReferenceClock(..) => NativeEvent::ReferenceClock(result),
            Self::Engine(_) | Self::TxGate(_) | Self::Radio(_) => {
                unreachable!("invariant: group steps end through their group's run")
            }
        }
    }
}

/// What a sequence's current action waits for.
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

/// Where a stream is, as the handle sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StreamState {
    /// Not open; it may open.
    Closed,
    /// Open: RX delivers blocks, TX takes sends.
    Open,
    /// An open or close failed, or RX failed: it must close before it opens again.
    Failed,
}

/// A uSDR's USB control plane and streams driven by a caller-owned Mio reactor (feature
/// `mio`), made by [`UsbBus::into_native`](super::super::UsbBus::into_native) from a bare
/// bus, or by [`Device::into_native`](crate::Device::into_native) from a powered, set-up
/// device.
///
/// Unstable pilot: it reads the TMP114 temperature and the gateware identity, switches
/// the reference clock, powers the board up behind a thermal policy and down, sets the
/// sample rate, tunes, sets bandwidths, gains and paths, creates, starts and stops a stream
/// group (the RF chains' power, tuning and DC calibration, the engine registers and DSP
/// corrections), mutes the TX core's lanes and throttles RX bursts, all as
/// [`Device`](crate::Device) does them, with the same portable machines, and runs the RX
/// and TX streams at the transport (the blocks on endpoints 0x83 and 0x03, framed and
/// throttled as the blocking bus frames them). It keeps the board's RF state as the
/// device does: a handed-over device's, or its own after
/// [`start_power_up`](Self::start_power_up). Its RF settings and power-down need that
/// state; a stream group without it configures only the stream engine, leaving the RF
/// chains as whatever powered the board left them. The reactor thread only
/// registers, schedules and observes; a worker thread owns the USB queues and performs
/// every USB call, stream cancellation and cleanup included. Waits inside an operation are
/// deadlines the caller's reactor honours, never sleeps.
///
/// Drive it from your event loop: pass every event for its token to
/// [`on_ready`](Self::on_ready), then call [`poll`](Self::poll) until it returns `None`
/// after any event for its token or the shared [`Waker`], and once
/// [`deadline`](Self::deadline) passes. Pass a `now` that never decreases across calls; a
/// step backwards fails the operation. One operation (a control operation, a stream's open
/// or close, or a send) is in flight at a time; RX blocks arrive alongside it. Any
/// control failure closes the handle, except refusing a second operation, a rejected
/// argument, or a failure a sequence finds in well-formed replies (such as
/// [`Error::ClockInputMissing`]); open the device again to continue. Stream failures end
/// only their stream. Unqualified on hardware.
pub struct NativeUsb {
    /// Registration, deadlines and the worker's mailboxes.
    adapter: Adapter,
    /// Carried over from the blocking bus.
    session: Session,
    /// Origin of the machines' caller-supplied time.
    epoch: Instant,
    /// The operation in flight.
    pending: Option<Pending>,
    /// The RX stream.
    rx: StreamState,
    /// The TX stream.
    tx: StreamState,
    /// The stream group, from its first stream's transport open until it stops.
    group: Option<NativeGroup>,
}

impl fmt::Debug for NativeUsb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeUsb")
            .field("pending", &self.pending.is_some())
            .field("rx", &self.rx)
            .field("tx", &self.tx)
            .field("group", &self.group)
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
            rx: StreamState::Closed,
            tx: StreamState::Closed,
            group: None,
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
            Ok(machine) => {
                self.pending = Some(Pending::Temperature(read, machine));
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
        let input = Board::si5332_reference(reference, self.session.board.rev3);
        let switch = ClockSequence::reference_switch(Si5332::usdr().dev(), input);
        self.start_sequence(SequenceOperation::ReferenceClock(switch, reference), now)
    }

    /// Starts a sequence operation's first action, refusing it while another is pending.
    fn start_sequence(&mut self, operation: SequenceOperation, now: Instant) -> Result<(), Error> {
        if self.pending.is_some() {
            return Err(Error::Busy);
        }
        let (operation, wait) = self.begin_sequence(operation, now)?;
        self.pending = Some(Pending::Sequence(operation, wait));
        Ok(())
    }

    /// Starts a sequence's first action. A failure to start it ends the session.
    fn begin_sequence(
        &mut self,
        mut operation: SequenceOperation,
        now: Instant,
    ) -> Result<(SequenceOperation, Wait), Error> {
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
        Ok((operation, wait))
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
                self.i2c(access, read, now).map(Wait::Transfer)
            }
            SequenceAction::Fpga(access) => {
                let id = self.take_id()?;
                let transaction = RegisterTransaction::access(id, access, self.elapsed(now))?;
                self.launch(Machine::Register(transaction), now)
                    .map(Wait::Transfer)
            }
        }
    }

    /// Starts an I²C register access reading `read` bytes back.
    fn i2c(
        &mut self,
        access: RegisterAccess,
        read: usize,
        now: Instant,
    ) -> Result<Machine, BusError> {
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
    fn launch(&mut self, mut machine: Machine, now: Instant) -> Result<Machine, BusError> {
        let Step::Submit(submission) = machine.start(self.elapsed(now))? else {
            return Err(BusError::Unsupported(
                "native USB machine finished without a transfer",
            ));
        };
        self.adapter.submit(&submission, now)?;
        Ok(machine)
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
        self.adapter.on_ready(token);
    }

    /// Observes the worker and deadlines once, starts the operation's next action, and
    /// returns one event: the operation's end, else an RX block. Call it again until it
    /// returns `None`; it never waits or performs USB I/O.
    #[must_use]
    pub fn poll(&mut self, now: Instant) -> Option<NativeEvent> {
        self.poll_operation(now).or_else(|| self.poll_rx())
    }

    /// Advances the operation in flight; its event once it ends.
    fn poll_operation(&mut self, now: Instant) -> Option<NativeEvent> {
        match self.pending.take()? {
            Pending::Temperature(read, mut machine) => {
                let Some(outcome) = self.observe(&mut machine, now) else {
                    self.pending = Some(Pending::Temperature(read, machine));
                    return None;
                };
                Some(self.conclude(read, outcome))
            }
            Pending::Sequence(operation, wait) => self.poll_sequence(operation, wait, now),
            Pending::Stream(operation) => self.poll_stream(operation),
            Pending::Send(send) => self.poll_send(send, now),
            Pending::Group(run) => self.poll_group(run, now),
            Pending::Thermal(read, machine, plan) => self.poll_thermal(read, machine, plan, now),
        }
    }

    /// Advances a sequence past its current action, once that has an outcome.
    fn poll_sequence(
        &mut self,
        operation: SequenceOperation,
        wait: Wait,
        now: Instant,
    ) -> Option<NativeEvent> {
        match self.progress(operation, wait, now) {
            Progress::Running(operation, wait) => {
                self.pending = Some(Pending::Sequence(operation, wait));
                None
            }
            Progress::Finished(SequenceOperation::Radio(run), result) => {
                Some(self.finish_radio(run, result))
            }
            Progress::Finished(operation, result) => {
                if let SequenceOperation::ReferenceClock(_, reference) = operation {
                    // As the blocking board: a missing input clock is reported after every
                    // write, so later plans use the new reference's frequency.
                    let switched = matches!(result, Ok(()) | Err(Error::ClockInputMissing));
                    if let (true, Some(radio)) = (switched, &mut self.session.board.radio) {
                        radio.reference_hz = reference.hz();
                    }
                }
                Some(operation.finish(result))
            }
        }
    }

    /// Advances a sequence past its current action, if that has an outcome yet, and starts
    /// its next.
    fn progress(
        &mut self,
        mut operation: SequenceOperation,
        mut wait: Wait,
        now: Instant,
    ) -> Progress {
        let outcome = match &mut wait {
            Wait::Sleep(until) => (now >= *until).then_some(Ok(None)),
            Wait::Transfer(machine) => self.observe(machine, now),
        };
        let Some(outcome) = outcome else {
            return Progress::Running(operation, wait);
        };
        match self.advance(&mut operation, outcome, now) {
            Ok(Some(wait)) => Progress::Running(operation, wait),
            Ok(None) => Progress::Finished(operation, Ok(())),
            Err(error) => Progress::Finished(operation, Err(error)),
        }
    }

    /// A machine's outcome, once there is one: its output, or the failure of its transfer.
    fn observe(
        &mut self,
        machine: &mut Machine,
        now: Instant,
    ) -> Option<Result<Option<Output>, BusError>> {
        let completion = self.adapter.try_event(now)?;
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
        NativeEvent::Temperature(result.and_then(|celsius| self.check_stop(celsius)))
    }

    /// `celsius`, or [`Error::Overheated`] when a powered board with a thermal policy has
    /// reached the policy's stop limit.
    fn check_stop(&self, celsius: f32) -> Result<f32, Error> {
        let (Some(_), Some(policy)) = (self.session.board.radio, self.session.thermal) else {
            return Ok(celsius);
        };
        let limit = policy.limits().stop();
        if celsius >= limit {
            return Err(Error::Overheated { celsius, limit });
        }
        Ok(celsius)
    }

    /// The machines' time for `now`.
    fn elapsed(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.epoch)
    }

    /// When [`poll`](Self::poll) must run even without an event.
    #[must_use]
    pub fn deadline(&self) -> Option<Instant> {
        match &self.pending {
            Some(Pending::Sequence(_, Wait::Sleep(until))) => Some(*until),
            Some(Pending::Send(send)) => send.deadline().or_else(|| self.adapter.deadline()),
            Some(Pending::Group(run)) => run.deadline().or_else(|| self.adapter.deadline()),
            _ => self.adapter.deadline(),
        }
    }

    /// Deregisters and lets the worker cancel every transfer, streams included, release the
    /// device and report through [`try_cleanup`](Self::try_cleanup); never waits.
    ///
    /// # Errors
    ///
    /// The deregistration failure. `NotFound` has still retired the worker; after any other
    /// error the token stays registered and `close` may be retried.
    pub fn close(&mut self) -> io::Result<()> {
        self.pending = None;
        self.rx = StreamState::Closed;
        self.tx = StreamState::Closed;
        self.group = None;
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
