//! The stream group on [`NativeUsb`]: create, start, host-ready, TX mute and remap, RX
//! throttle and stop, as the blocking device's stream group performs them
//! (`board/stream.rs`, `board/tx_stream.rs`). Each operation is a list of steps run one at a time: the RF
//! chains' part (power, the default tuning and DC calibration, from the handle's RF
//! state), the same portable engine sequences, DSP-port writes and gateware gate, each
//! stream's transport open or close, and the refusals the blocking device makes between
//! them.

use std::collections::VecDeque;
use std::mem;
use std::time::Instant;

use super::super::window::Windowed;
use super::super::worker::StreamCommand;
use super::stream::StreamOperation;
use super::{NativeEvent, NativeUsb, Pending, Progress, SequenceOperation, StreamState, Wait};
use crate::board::{RxThrottle, TxLaneUse};
use crate::error::Error;
use crate::fpga::{self, BurstPlan, IdentityRead, Phy};
use crate::lowlevel::BusError;
use crate::portable::radio::Chain;
use crate::portable::stream::{EngineSequence, SyncTrigger};
use crate::stream::{StreamsConfig, SyncMode, TxChannels, TxMute, WireFormat};

/// The stream group the handle holds: which streams joined it and how it starts.
#[derive(Clone, Copy, Debug)]
pub(super) struct NativeGroup {
    /// The RX stream's transport opened.
    rx: bool,
    /// The TX stream's transport opened.
    tx: bool,
    /// How the group starts.
    mode: SyncMode,
    /// How the TX stream uses the TX core's lanes, which starting the group applies: as
    /// the blocking stream is created for its channels, until a mute changes it (on the
    /// original core).
    tx_lanes: TxLaneUse,
    /// The group has started.
    started: bool,
}

impl NativeGroup {
    /// The group holds the RX stream.
    pub(super) const fn holds_rx(self) -> bool {
        self.rx
    }

    /// The group holds the TX stream.
    pub(super) const fn holds_tx(self) -> bool {
        self.tx
    }
}

/// Which group operation a run performs, and so how it ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GroupKind {
    /// Create: the first failure ends it.
    Create,
    /// Start.
    Start,
    /// Tell the RX DMA the host is ready.
    RxReady,
    /// Mute the TX core's lanes.
    TxMute,
    /// Move the TX stream to other channels.
    TxChannels,
    /// Throttle the RX front end's bursts.
    RxThrottle,
    /// Stop: every step is attempted; the first failure is reported.
    Stop,
}

/// One step of a group operation.
#[expect(
    clippy::large_enum_variant,
    reason = "a run's few steps, each a fixed-size machine; boxing would allocate per step"
)]
enum GroupStep {
    /// Run a portable sequence.
    Sequence(SequenceOperation),
    /// Hand a stream command to the worker.
    Stream(StreamOperation, StreamCommand),
    /// Fail here, where the blocking device refuses (a packet size its checks reach only
    /// after some I/O).
    Fail(Error),
    /// The RF chains' part of a stream's create (`true`) or stop, built from the handle's
    /// RF state when it begins; skipped on a handle without one.
    Radio(Chain, bool),
}

/// The step in flight.
#[expect(
    clippy::large_enum_variant,
    reason = "one per handle; boxing the machine would allocate per action"
)]
enum Current {
    /// None yet: the next poll from this instant starts the next step.
    Idle(Instant),
    /// A sequence, at its current action.
    Sequence(SequenceOperation, Wait),
    /// A stream command, awaiting the worker.
    Stream(StreamOperation),
}

/// A group operation in flight.
pub(super) struct GroupRun {
    /// What it does.
    kind: GroupKind,
    /// How a group it creates starts.
    mode: SyncMode,
    /// How the TX stream it creates uses the TX core's lanes.
    tx_lanes: TxLaneUse,
    /// The steps not yet begun.
    steps: VecDeque<GroupStep>,
    /// The step in flight.
    current: Current,
    /// The first failure.
    error: Option<Error>,
}

impl GroupRun {
    /// A run of `steps`, to begin at the next poll from `now`.
    fn new(kind: GroupKind, mode: SyncMode, steps: VecDeque<GroupStep>, now: Instant) -> Self {
        Self {
            kind,
            mode,
            tx_lanes: TxLaneUse::default(),
            steps,
            current: Current::Idle(now),
            error: None,
        }
    }

    /// When the handle must poll without an event: at once before a step, or at the end
    /// of a sequence's wait.
    pub(super) const fn deadline(&self) -> Option<Instant> {
        match self.current {
            Current::Idle(at) | Current::Sequence(_, Wait::Sleep(at)) => Some(at),
            Current::Sequence(_, Wait::Transfer(_)) | Current::Stream(_) => None,
        }
    }
}

/// An engine sequence step, through the index window.
fn engine(sequence: EngineSequence) -> GroupStep {
    GroupStep::Sequence(SequenceOperation::Engine(Windowed::new(sequence)))
}

impl NativeUsb {
    /// Starts creating the stream group `config` describes, as
    /// [`Device::create_streams`](crate::Device::create_streams) does, RX first: per
    /// stream, the RF chain's part (RX: the receiver powered, tuned to 320 MHz if no
    /// frequency was set, its DC offsets calibrated and the TX PLL tuned back; TX: the
    /// transmitter powered), then the engine's setup (RX: the DMA's liveness check and reset,
    /// the front end's and DMA's burst configuration for the stream's channels and the
    /// sync unit held stopped; TX: on the extended TX core its front-end reset, the
    /// gateware's minimum build, on the extended core its front end set up for the
    /// stream's lanes and sample width, then the sync unit held stopped), then the
    /// stream's transport opens, the stream joins the group (TX using the core's lanes as
    /// the blocking stream does: on the original core one channel mutes the other lane),
    /// and the DSP chain's port is written as the blocking create writes it (RX: its IQ
    /// correction
    /// again; TX: the constant generator off, then its IQ correction again, the
    /// corrections a handed-over device carried on). [`poll`](Self::poll) reports
    /// [`NativeEvent::StreamsCreated`]; RX blocks then arrive as
    /// [`NativeEvent::Received`] once the group starts, and TX takes
    /// [`start_send`](Self::start_send).
    ///
    /// On a handle without the board's RF state (from a bare bus, not powered up by it) the
    /// RF parts are left out, and the chains stay as whatever powered the board left them.
    ///
    /// # Errors
    ///
    /// Before any I/O, refusing it as the blocking device does: [`Error::Busy`] if an
    /// operation is pending; [`Error::AlreadyStreaming`] while a group exists or either
    /// stream is open, or failed and not yet closed, at the transport; [`Error::NoStreams`],
    /// [`Error::UnsupportedSync`] and the channel, format and packet refusals of
    /// [`Device::create_streams`](crate::Device::create_streams);
    /// [`Error::Stream`] if the handle is closed.
    pub fn start_create_streams(
        &mut self,
        config: StreamsConfig,
        now: Instant,
    ) -> Result<(), Error> {
        self.refuse_group_operation()?;
        if self.group.is_some() || self.rx != StreamState::Closed || self.tx != StreamState::Closed
        {
            return Err(Error::AlreadyStreaming);
        }
        let chains = self.session.board.chains;
        config.check_on(chains)?;
        let mut steps = VecDeque::new();
        if let Some(rx) = config.rx {
            steps.push_back(GroupStep::Radio(Chain::Rx, true));
            // libusdr checks the packet size between the DMA reset and the configuration.
            steps.push_back(engine(EngineSequence::rx_reset()));
            match BurstPlan::for_stream(rx.samples_per_packet, rx.wire, rx.channels) {
                Some(plan) => {
                    let format = plan.rx_format(rx.channels);
                    steps.push_back(engine(EngineSequence::rx_configure(&format)));
                    steps.push_back(GroupStep::Stream(
                        StreamOperation::RxOpen,
                        StreamCommand::RxOpen(plan.block_bytes()),
                    ));
                }
                None => steps.push_back(GroupStep::Fail(Error::UnsupportedPacketSize(
                    rx.samples_per_packet,
                ))),
            }
            steps.push_back(engine(Phy::rx_create_writes(
                self.session.board.corrections.rx,
            )));
        }
        let mut tx_lanes = TxLaneUse::default();
        if let Some(tx) = config.tx {
            let extended = chains.extended_tx_core;
            tx_lanes = TxLaneUse::created(tx.channels, extended);
            steps.push_back(GroupStep::Radio(Chain::Tx, true));
            if extended {
                steps.push_back(engine(Phy::tx_extended_reset_writes()));
            }
            let gate = SequenceOperation::TxGate(IdentityRead::default());
            steps.push_back(GroupStep::Sequence(gate));
            if extended {
                let twelve_bit = tx.wire == WireFormat::Ci12;
                let map = tx.channels.lane_map();
                steps.push_back(engine(EngineSequence::extended_tx_configure(
                    map, twelve_bit,
                )));
            }
            steps.push_back(engine(EngineSequence::new().sync(SyncTrigger::Stop)));
            let sample_bytes = tx.sample_bytes();
            steps.push_back(
                match fpga::stream_block_bytes(tx.samples_per_packet, sample_bytes) {
                    Ok(block_bytes) => GroupStep::Stream(
                        StreamOperation::TxOpen,
                        StreamCommand::TxOpen {
                            block_bytes,
                            sample_bytes,
                        },
                    ),
                    Err(error) => GroupStep::Fail(error),
                },
            );
            steps.push_back(engine(Phy::tx_create_writes(
                self.session.board.corrections.tx,
            )));
        }
        let mut run = GroupRun::new(GroupKind::Create, config.sync, steps, now);
        run.tx_lanes = tx_lanes;
        self.pending = Some(Pending::Group(run));
        Ok(())
    }

    /// Starts the group, as [`Device::start_streams`](crate::Device::start_streams) does:
    /// RX alone syncs, then runs; with TX, RX runs, then the TX core starts with the
    /// group's mute (lane B from the create, as the blocking stream's, unless
    /// [`start_tx_mute`](Self::start_tx_mute) changed it), then the group syncs.
    /// [`poll`](Self::poll) reports [`NativeEvent::StreamsStarted`].
    ///
    /// # Errors
    ///
    /// Before any I/O: [`Error::Busy`] if an operation is pending; [`Error::NotStreaming`]
    /// without a group; [`Error::AlreadyStreaming`] once it started;
    /// [`Error::UnsupportedSync`] for a mode with no trigger for the streams it holds;
    /// [`Error::Stream`] if the handle is closed.
    pub fn start_streams(&mut self, now: Instant) -> Result<(), Error> {
        self.refuse_group_operation()?;
        let Some(group) = self.group else {
            return Err(Error::NotStreaming);
        };
        if group.started {
            return Err(Error::AlreadyStreaming);
        }
        let trigger = group.mode.trigger(group.rx, group.tx)?;
        let tx_lanes = group.tx.then(|| group.tx_lanes.lanes());
        let start = engine(EngineSequence::start_lanes(group.rx, tx_lanes, trigger));
        let run = GroupRun::new(GroupKind::Start, group.mode, VecDeque::from([start]), now);
        self.pending = Some(Pending::Group(run));
        Ok(())
    }

    /// Starts telling the RX DMA engine the host is ready, as
    /// [`Device::set_rx_ready`](crate::Device::set_rx_ready) does (libusdr's
    /// `usdr_dms_set_ready`); the blocking device's receive does so itself until its first
    /// packet, which the native handle, delivering raw blocks, leaves to its caller.
    /// [`poll`](Self::poll) reports [`NativeEvent::RxReady`].
    ///
    /// # Errors
    ///
    /// Before any I/O: [`Error::Busy`] if an operation is pending; [`Error::NotStreaming`]
    /// without an RX stream in the group; [`Error::Stream`] if the handle is closed.
    pub fn start_rx_ready(&mut self, now: Instant) -> Result<(), Error> {
        self.refuse_group_operation()?;
        let Some(group @ NativeGroup { rx: true, .. }) = self.group else {
            return Err(Error::NotStreaming);
        };
        let ready = engine(EngineSequence::new().signal_rx_ready());
        let run = GroupRun::new(GroupKind::RxReady, group.mode, VecDeque::from([ready]), now);
        self.pending = Some(Pending::Group(run));
        Ok(())
    }

    /// Starts muting the TX core's lanes as `mute` says, as
    /// [`Device::set_tx_mute`](crate::Device::set_tx_mute) does (libusdr's stream option
    /// `"mute"`): on the original TX core the group keeps the setting, which its start
    /// applies, and the core takes it now; on the extended core it goes to the core's
    /// front end, raw, and the start mutes nothing. [`poll`](Self::poll) reports
    /// [`NativeEvent::TxMuted`].
    ///
    /// # Errors
    ///
    /// Before any I/O: [`Error::Busy`] if an operation is pending; [`Error::NotStreaming`]
    /// without a TX stream in the group; [`Error::Stream`] if the handle is closed.
    pub fn start_tx_mute(&mut self, mute: TxMute, now: Instant) -> Result<(), Error> {
        self.refuse_group_operation()?;
        let extended = self.session.board.chains.extended_tx_core;
        let Some(group) = self.group.as_mut().filter(|group| group.tx) else {
            return Err(Error::NotStreaming);
        };
        let update = if extended {
            EngineSequence::new().extended_tx_mute(mute.bits().into())
        } else {
            group.tx_lanes.mute = mute;
            EngineSequence::new().update_tx_lanes(group.tx_lanes.lanes())
        };
        let update = engine(update);
        let run = GroupRun::new(GroupKind::TxMute, group.mode, VecDeque::from([update]), now);
        self.pending = Some(Pending::Group(run));
        Ok(())
    }

    /// Starts moving the TX stream to `channels`, as many as it carries, as
    /// [`Device::set_tx_channels`](crate::Device::set_tx_channels) does (libusdr's stream
    /// option `"chmap"`), with the same lane rule: on the original TX core a one-channel
    /// stream's lanes swap and a lane mute moves to the lane left idle, which the group
    /// keeps for its start; on the extended core the lane map is rewritten.
    /// [`poll`](Self::poll) reports [`NativeEvent::TxChannelsSet`].
    ///
    /// Unstable, pending owner review, as the blocking method is.
    ///
    /// # Errors
    ///
    /// Before any I/O: [`Error::Busy`] if an operation is pending; [`Error::NotStreaming`]
    /// without a TX stream in the group; [`Error::UnsupportedTxChannels`] for another
    /// channel count or, without the TX chain, channel 1; [`Error::Stream`] if the handle
    /// is closed.
    pub fn start_tx_channels(&mut self, channels: TxChannels, now: Instant) -> Result<(), Error> {
        self.refuse_group_operation()?;
        let chains = self.session.board.chains;
        let Some(group) = self.group.as_mut().filter(|group| group.tx) else {
            return Err(Error::NotStreaming);
        };
        let update = group
            .tx_lanes
            .remap(channels, chains.extended_tx_core, chains.tx)?;
        let run = GroupRun::new(
            GroupKind::TxChannels,
            group.mode,
            VecDeque::from([engine(update)]),
            now,
        );
        self.pending = Some(Pending::Group(run));
        Ok(())
    }

    /// Starts throttling the RX front end's bursts (`Some`) or delivering them all
    /// (`None`), as [`Device::set_rx_throttle`](crate::Device::set_rx_throttle) does
    /// (libusdr's `/dm/sdr/0/rfe/throttle`). [`poll`](Self::poll) reports
    /// [`NativeEvent::RxThrottled`].
    ///
    /// # Errors
    ///
    /// Before any I/O: [`Error::Busy`] if an operation is pending; [`Error::NotStreaming`]
    /// without an RX stream in the group; [`Error::Stream`] if the handle is closed.
    pub fn start_rx_throttle(
        &mut self,
        throttle: Option<RxThrottle>,
        now: Instant,
    ) -> Result<(), Error> {
        self.refuse_group_operation()?;
        let Some(group @ NativeGroup { rx: true, .. }) = self.group else {
            return Err(Error::NotStreaming);
        };
        let throttle = throttle.map(|RxThrottle { send, skip }| (send, skip));
        let write = engine(EngineSequence::new().throttle_rx(throttle));
        let run = GroupRun::new(
            GroupKind::RxThrottle,
            group.mode,
            VecDeque::from([write]),
            now,
        );
        self.pending = Some(Pending::Group(run));
        Ok(())
    }

    /// Starts stopping and destroying the group, as
    /// [`Device::stop_streams`](crate::Device::stop_streams) does on the stream engine:
    /// RX halts, then TX stops; then each stream's destroy powers its chain down, halts it
    /// again and closes its transport, the worker cancelling every transfer. Every step is attempted; the group
    /// is gone at once. [`poll`](Self::poll) reports [`NativeEvent::StreamsStopped`].
    ///
    /// # Errors
    ///
    /// Before any I/O: [`Error::Busy`] if an operation is pending; [`Error::NotStreaming`]
    /// without a group; [`Error::Stream`] if the handle is closed.
    pub fn start_stop_streams(&mut self, now: Instant) -> Result<(), Error> {
        self.refuse_group_operation()?;
        let Some(group) = self.group.take() else {
            return Err(Error::NotStreaming);
        };
        let mut steps = VecDeque::new();
        if group.rx {
            steps.push_back(engine(EngineSequence::new().run_rx(false)));
        }
        if group.tx {
            steps.push_back(engine(EngineSequence::new().stop_tx()));
        }
        if group.rx {
            steps.push_back(GroupStep::Radio(Chain::Rx, false));
            steps.push_back(engine(EngineSequence::new().run_rx(false)));
            steps.push_back(GroupStep::Stream(
                StreamOperation::RxClose,
                StreamCommand::RxClose,
            ));
        }
        if group.tx {
            steps.push_back(GroupStep::Radio(Chain::Tx, false));
            steps.push_back(engine(EngineSequence::new().stop_tx()));
            steps.push_back(GroupStep::Stream(
                StreamOperation::TxClose,
                StreamCommand::TxClose,
            ));
        }
        let run = GroupRun::new(GroupKind::Stop, group.mode, steps, now);
        self.pending = Some(Pending::Group(run));
        Ok(())
    }

    /// The refusals every group operation makes first.
    fn refuse_group_operation(&self) -> Result<(), Error> {
        if self.pending.is_some() {
            return Err(Error::Busy);
        }
        if self.adapter.is_stopped() {
            return Err(Error::Stream(BusError::Disconnected));
        }
        Ok(())
    }

    /// Advances a group operation through as many steps as have outcomes; its event once
    /// it ends.
    pub(super) fn poll_group(&mut self, mut run: GroupRun, now: Instant) -> Option<NativeEvent> {
        loop {
            let outcome = match mem::replace(&mut run.current, Current::Idle(now)) {
                Current::Idle(_) => {
                    let Some(step) = run.steps.pop_front() else {
                        return Some(self.finish_group(run));
                    };
                    match step {
                        GroupStep::Sequence(operation) => {
                            match self.begin_sequence(operation, now) {
                                Ok((operation, wait)) => {
                                    run.current = Current::Sequence(operation, wait);
                                    continue;
                                }
                                Err(error) => Err(error),
                            }
                        }
                        GroupStep::Stream(operation, command) => {
                            match self.begin_stream(operation, command, now) {
                                Ok(()) => {
                                    run.current = Current::Stream(operation);
                                    continue;
                                }
                                Err(error) => Err(error),
                            }
                        }
                        GroupStep::Fail(error) => Err(error),
                        GroupStep::Radio(chain, up) => {
                            let Some(operation) = self.group_radio_step(chain, up) else {
                                continue;
                            };
                            match self.begin_sequence(operation, now) {
                                Ok((operation, wait)) => {
                                    run.current = Current::Sequence(operation, wait);
                                    continue;
                                }
                                Err(error) => Err(error),
                            }
                        }
                    }
                }
                Current::Sequence(operation, wait) => match self.progress(operation, wait, now) {
                    Progress::Running(operation, wait) => {
                        run.current = Current::Sequence(operation, wait);
                        self.pending = Some(Pending::Group(run));
                        return None;
                    }
                    Progress::Finished(SequenceOperation::Radio(radio), result) => {
                        self.conclude_radio(radio, result)
                    }
                    Progress::Finished(operation, result) => operation.step_result(result),
                },
                Current::Stream(operation) => {
                    let Some(result) = self.command_result(now) else {
                        run.current = Current::Stream(operation);
                        self.pending = Some(Pending::Group(run));
                        return None;
                    };
                    let result = self.settle_stream(operation, result);
                    if result.is_ok() {
                        self.join(run.mode, run.tx_lanes, operation);
                    }
                    result
                }
            };
            if let Err(error) = outcome {
                run.error.get_or_insert(error);
                if run.kind != GroupKind::Stop {
                    return Some(self.finish_group(run));
                }
            }
        }
    }

    /// A stream whose transport opened joins the group, made to start with `mode` and a
    /// TX stream's `tx_lanes` if this is its first stream.
    fn join(&mut self, mode: SyncMode, tx_lanes: TxLaneUse, operation: StreamOperation) {
        let group = || NativeGroup {
            rx: false,
            tx: false,
            mode,
            tx_lanes,
            started: false,
        };
        match operation {
            StreamOperation::RxOpen => self.group.get_or_insert_with(group).rx = true,
            StreamOperation::TxOpen => self.group.get_or_insert_with(group).tx = true,
            StreamOperation::RxClose | StreamOperation::TxClose => {}
        }
    }

    /// The event for a finished run.
    fn finish_group(&mut self, run: GroupRun) -> NativeEvent {
        let result = run.error.map_or(Ok(()), Err);
        match run.kind {
            GroupKind::Create => NativeEvent::StreamsCreated(result),
            GroupKind::Start => {
                if let (Ok(()), Some(group)) = (&result, &mut self.group) {
                    group.started = true;
                }
                NativeEvent::StreamsStarted(result)
            }
            GroupKind::RxReady => NativeEvent::RxReady(result),
            GroupKind::TxMute => NativeEvent::TxMuted(result),
            GroupKind::TxChannels => NativeEvent::TxChannelsSet(result),
            GroupKind::RxThrottle => NativeEvent::RxThrottled(result),
            GroupKind::Stop => NativeEvent::StreamsStopped(result),
        }
    }
}
