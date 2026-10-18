//! The board's RF operations on [`NativeUsb`]: power-up behind the thermal policy and
//! power-down, the sample rate, tuning, bandwidths, gains and paths, and the RF chains'
//! part of a stream group's create and stop. Each runs the portable machine the blocking
//! [`Board`](crate::board) runs (`portable::radio`), one action at a time, with the
//! handle's copy of the board's RF state, taken back when it ends.

use std::time::Instant;

use super::super::super::machine::{Machine, Output};
use super::{NativeEvent, NativeUsb, Pending, SequenceOperation};
use crate::board::{Board, RxGain, RxPath, TxGain, TxPath};
use crate::chips::tmp114::{TemperatureRead, Tmp114};
use crate::chips::tps6381x::Tps6381x;
use crate::error::Error;
use crate::lowlevel::BusError;
use crate::portable::fpga::FPGA;
use crate::portable::radio::run::OwnsRadio;
use crate::portable::radio::wiring;
use crate::portable::radio::{
    Chain, ChainPower, NcoChannels, PowerUp, PowerUpPlan, Radio, RateChains, Run, SampleRate,
    SetRxPath, StreamUp, Tune, power_down, stream_down,
};
use crate::portable::script::Script;
use crate::portable::sequence::{
    FpgaAccess, RegisterAccess, Sequence, SequenceAction, SequenceFailure,
};
use crate::portable::{lms6002d, lp8758, si5332, tps6381x};
use crate::reference::ReferenceClock;
use crate::thermal::ThermalPolicy;

/// A portable RF machine the handle runs.
pub(super) enum RadioOp {
    /// Power-up.
    PowerUp(Run<PowerUp>),
    /// A sample rate.
    Rate(Run<SampleRate>),
    /// A tuning.
    Tune(Run<Tune>),
    /// An RX path.
    RxPath(Run<SetRxPath>),
    /// A chain's power: a stream's RF part on stop.
    Power(Run<ChainPower>),
    /// A stream's RF part on create.
    Up(Run<StreamUp>),
    /// Straight-line writes whose state the handle recorded when it began them.
    Script(Script),
}

impl RadioOp {
    /// The machine's next action.
    fn next(&self) -> Option<SequenceAction> {
        match self {
            Self::PowerUp(run) => run.next(),
            Self::Rate(run) => run.next(),
            Self::Tune(run) => run.next(),
            Self::RxPath(run) => run.next(),
            Self::Power(run) => run.next(),
            Self::Up(run) => run.next(),
            Self::Script(script) => script.next(),
        }
    }

    /// Applies an action's outcome.
    fn complete(&mut self, result: Result<u32, BusError>) -> Result<(), SequenceFailure<BusError>> {
        match self {
            Self::PowerUp(run) => run.complete(result),
            Self::Rate(run) => run.complete(result),
            Self::Tune(run) => run.complete(result),
            Self::RxPath(run) => run.complete(result),
            Self::Power(run) => run.complete(result),
            Self::Up(run) => run.complete(result),
            Self::Script(script) => script.complete(result),
        }
    }

    /// The RF state the machine leaves: `None` for writes, whose state was recorded when
    /// they began, and for a power-up that did not finish.
    fn radio(&self) -> Option<Radio> {
        match self {
            Self::PowerUp(run) => run.steps().radio(),
            Self::Rate(run) => Some(*run.steps().radio()),
            Self::Tune(run) => Some(*run.steps().radio()),
            Self::RxPath(run) => Some(*run.steps().radio()),
            Self::Power(run) => Some(*run.steps().radio()),
            Self::Up(run) => Some(*run.steps().radio()),
            Self::Script(_) => None,
        }
    }

    /// The tuning fault that ended the machine, if one did.
    fn fault(&self) -> Option<Error> {
        let fault = match self {
            Self::Rate(run) => run.fault(),
            Self::Tune(run) => run.fault(),
            Self::Up(run) => run.fault(),
            Self::PowerUp(_) | Self::RxPath(_) | Self::Power(_) | Self::Script(_) => None,
        };
        fault.map(Error::from)
    }
}

/// What a finished RF operation reports as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RadioKind {
    /// [`NativeEvent::PoweredUp`].
    PowerUp,
    /// [`NativeEvent::PoweredDown`].
    PowerDown,
    /// [`NativeEvent::SampleRateSet`].
    SampleRate,
    /// [`NativeEvent::Tuned`].
    Tune,
    /// [`NativeEvent::BandwidthSet`].
    Bandwidth,
    /// [`NativeEvent::GainSet`].
    Gain,
    /// [`NativeEvent::PathSet`].
    Path,
    /// [`NativeEvent::CorrectionSet`].
    Correction,
    /// A step of a stream group's create or stop.
    GroupStep,
}

/// An RF operation in flight: its machine, how it reports, and a failure it went on past.
pub(super) struct RadioRun {
    /// The machine.
    op: RadioOp,
    /// How it reports.
    kind: RadioKind,
    /// A failure the machine reported and went on past (its remaining steps are attempted,
    /// as on the blocking board); the run fails with it once they ran.
    first: Option<Error>,
}

impl RadioRun {
    /// `op`, reporting as `kind`.
    pub(super) const fn new(op: RadioOp, kind: RadioKind) -> Self {
        Self {
            op,
            kind,
            first: None,
        }
    }

    /// The machine's next action, as [`Sequence::next`].
    pub(super) fn next(&self) -> Option<SequenceAction> {
        self.op.next()
    }

    /// Reports an action's outcome. A failure the machine goes on past is kept and the run
    /// continues, unless it is a transfer failure, which ends the session: the handle
    /// cannot attempt more, so the run ends with it at once, even where the machine would
    /// ignore it.
    pub(super) fn complete(&mut self, result: Result<u32, BusError>) -> Result<(), Error> {
        let result = match (result, self.op.next()) {
            (Err(source), Some(action)) => {
                let failure = transfer_failure(action, source);
                return Err(self.first.take().unwrap_or(failure));
            }
            (result, _) => result,
        };
        let Err(failure) = self.op.complete(result) else {
            return Ok(());
        };
        let transfer = matches!(
            failure,
            SequenceFailure::Transport { .. } | SequenceFailure::FpgaTransport { .. }
        );
        let error = Error::from(failure);
        if transfer || self.op.next().is_none() {
            return Err(self.first.take().unwrap_or(error));
        }
        self.first.get_or_insert(error);
        Ok(())
    }
}

/// The failure of `action`'s transfer: the access and the chip the uSDR wires there.
fn transfer_failure(action: SequenceAction, source: BusError) -> Error {
    let failure = match action {
        SequenceAction::Fpga(access @ FpgaAccess::Spi { .. }) => SequenceFailure::FpgaTransport {
            chip: lms6002d::chip::NAME,
            access,
            source,
        },
        SequenceAction::Fpga(access) => SequenceFailure::FpgaTransport {
            chip: FPGA,
            access,
            source,
        },
        SequenceAction::Access(access) => {
            let (RegisterAccess::Read { dev, .. } | RegisterAccess::Write { dev, .. }) = access;
            let chip = match dev {
                wiring::PMIC => lp8758::NAME,
                wiring::CLOCK => si5332::NAME,
                dev if dev == Tps6381x::usdr().dev() => tps6381x::NAME,
                _ => "TMP114",
            };
            SequenceFailure::Transport {
                chip,
                access,
                source,
            }
        }
        // A wait has no transfer.
        SequenceAction::Sleep(_) => return Error::Stream(source),
    };
    Error::from(failure)
}

impl NativeUsb {
    /// The handle's RF state, or [`Error::NotPowered`] before a power-up.
    fn radio(&self) -> Result<Radio, Error> {
        self.session.board.radio.ok_or(Error::NotPowered)
    }

    /// Starts an RF operation's first action, refusing it while another is pending.
    pub(super) fn start_radio(
        &mut self,
        op: RadioOp,
        kind: RadioKind,
        now: Instant,
    ) -> Result<(), Error> {
        self.start_sequence(SequenceOperation::Radio(RadioRun::new(op, kind)), now)
    }

    /// Takes back what a finished RF operation changed, and its result: its first failure,
    /// else the fault that ended it.
    pub(super) fn conclude_radio(
        &mut self,
        run: RadioRun,
        result: Result<(), Error>,
    ) -> Result<(), Error> {
        let RadioRun { op, kind, first } = run;
        match (kind, op.radio()) {
            (RadioKind::PowerUp, radio) => self.session.board.radio = radio,
            (_, Some(radio)) => self.session.board.radio = Some(radio),
            (_, None) => {}
        }
        let result = first.map_or(result, Err);
        match op.fault() {
            Some(fault) if result.is_ok() => Err(fault),
            _ => result,
        }
    }

    /// The event for a finished RF operation.
    pub(super) fn finish_radio(&mut self, run: RadioRun, result: Result<(), Error>) -> NativeEvent {
        let kind = run.kind;
        let result = self.conclude_radio(run, result);
        match kind {
            RadioKind::PowerUp => NativeEvent::PoweredUp(result),
            RadioKind::PowerDown => {
                self.session.board.radio = None;
                NativeEvent::PoweredDown(result)
            }
            RadioKind::SampleRate => NativeEvent::SampleRateSet(result),
            RadioKind::Tune => NativeEvent::Tuned(result),
            RadioKind::Bandwidth => NativeEvent::BandwidthSet(result),
            RadioKind::Gain => NativeEvent::GainSet(result),
            RadioKind::Path => NativeEvent::PathSet(result),
            RadioKind::Correction => NativeEvent::CorrectionSet(result),
            RadioKind::GroupStep => {
                unreachable!("invariant: group steps end through their group's run")
            }
        }
    }

    /// Starts powering the board up with its clocks' reference from `reference`, as
    /// [`DeviceBuilder::open`](crate::DeviceBuilder::open) does after identification:
    /// first the TMP114's temperature, refused at or above `policy`'s start limit (never
    /// above the 110 °C hard stop), then rails, clocks and the RF transceiver in its idle
    /// configuration, the blocking device's power-up machine. It does not wait for a hot
    /// board to cool (read [`start_temperature`](Self::start_temperature) and retry) and
    /// probes no front end, as a device opened with
    /// [`FrontEndSelection::None`](crate::FrontEndSelection::None). The handle keeps
    /// `policy`. [`poll`](Self::poll) reports [`NativeEvent::PoweredUp`].
    ///
    /// # Errors
    ///
    /// Before any I/O: [`Error::UnsupportedReferenceClock`]; [`Error::Busy`] if an
    /// operation is pending; [`Error::AlreadyPowered`] once powered (by this handle or the
    /// device that handed over); [`Error::Bus`] if the handle is closed.
    pub fn start_power_up(
        &mut self,
        reference: ReferenceClock,
        policy: ThermalPolicy,
        now: Instant,
    ) -> Result<(), Error> {
        reference.check()?;
        if self.pending.is_some() {
            return Err(Error::Busy);
        }
        if self.session.board.radio.is_some() {
            return Err(Error::AlreadyPowered);
        }
        if self.group.is_some() {
            return Err(Error::AlreadyStreaming);
        }
        let board = self.session.board;
        let plan = Board::power_up_plan(board.rev3, board.chains, reference);
        let (mut read, request) = TemperatureRead::begin(Tmp114::usdr());
        let access = RegisterAccess::Read {
            dev: request.dev,
            reg: request.register[0],
        };
        match self.i2c(access, 2, now) {
            Ok(machine) => {
                self.session.thermal = Some(policy);
                self.pending = Some(Pending::Thermal(read, machine, plan));
                Ok(())
            }
            Err(error) => {
                self.adapter.retire_after_completion();
                Err(read.fail(error))
            }
        }
    }

    /// Advances the power-up's temperature check: once read, refuses a board too hot for
    /// the policy, or begins the power-up.
    pub(super) fn poll_thermal(
        &mut self,
        mut read: TemperatureRead,
        mut machine: Machine,
        plan: PowerUpPlan,
        now: Instant,
    ) -> Option<NativeEvent> {
        let Some(outcome) = self.observe(&mut machine, now) else {
            self.pending = Some(Pending::Thermal(read, machine, plan));
            return None;
        };
        let celsius = match outcome {
            Ok(Some(Output::I2c { bytes, len })) => read.resolve(Ok(&bytes[..len])),
            Ok(_) => read.resolve(Err(BusError::Unsupported(
                "native temperature read completed without I2C bytes",
            ))),
            Err(error) => read.resolve(Err(error)),
        };
        let celsius = match celsius {
            Ok(celsius) => celsius,
            Err(error) => {
                self.adapter.retire_after_completion();
                return Some(NativeEvent::PoweredUp(Err(error)));
            }
        };
        let limits = self.session.thermal.unwrap_or_default().limits();
        if celsius >= limits.start() {
            return Some(NativeEvent::PoweredUp(Err(Error::Overheated {
                celsius,
                limit: limits.start(),
            })));
        }
        let run = RadioRun::new(
            RadioOp::PowerUp(Run::new(PowerUp::new(plan))),
            RadioKind::PowerUp,
        );
        match self.begin_sequence(SequenceOperation::Radio(run), now) {
            Ok((operation, wait)) => {
                self.pending = Some(Pending::Sequence(operation, wait));
                None
            }
            Err(error) => Some(NativeEvent::PoweredUp(Err(error))),
        }
    }

    /// Starts powering the board down, as [`Device::close`](crate::Device::close) does once
    /// no stream group is left: the RF transceiver held in reset, the LED and booster off,
    /// every write attempted. The handle then holds no RF state; power it up again to
    /// configure it. [`poll`](Self::poll) reports [`NativeEvent::PoweredDown`].
    ///
    /// # Errors
    ///
    /// Before any I/O: [`Error::Busy`] if an operation is pending; [`Error::NotPowered`]
    /// before a power-up; [`Error::AlreadyStreaming`] while a stream group exists;
    /// [`Error::Bus`] if the handle is closed.
    pub fn start_power_down(&mut self, now: Instant) -> Result<(), Error> {
        self.refuse_radio_operation()?;
        if self.group.is_some() {
            return Err(Error::AlreadyStreaming);
        }
        self.start_radio(RadioOp::Script(power_down()), RadioKind::PowerDown, now)
    }

    /// The refusals every RF setting makes first: an operation pending, no power-up.
    pub(super) fn refuse_radio_operation(&self) -> Result<Radio, Error> {
        if self.pending.is_some() {
            return Err(Error::Busy);
        }
        self.radio()
    }

    /// Starts setting the RX sample rate, as
    /// [`Device::set_rx_sample_rate`](crate::Device::set_rx_sample_rate) does;
    /// [`poll`](Self::poll) reports [`NativeEvent::SampleRateSet`].
    ///
    /// # Errors
    ///
    /// As [`start_sample_rate`](Self::start_sample_rate).
    pub fn start_rx_sample_rate(
        &mut self,
        samples_per_second: u32,
        now: Instant,
    ) -> Result<(), Error> {
        self.start_rate(samples_per_second, RateChains::Rx, now)
    }

    /// Starts setting the TX sample rate, as
    /// [`Device::set_tx_sample_rate`](crate::Device::set_tx_sample_rate) does;
    /// [`poll`](Self::poll) reports [`NativeEvent::SampleRateSet`].
    ///
    /// # Errors
    ///
    /// As [`start_sample_rate`](Self::start_sample_rate).
    pub fn start_tx_sample_rate(
        &mut self,
        samples_per_second: u32,
        now: Instant,
    ) -> Result<(), Error> {
        self.start_rate(samples_per_second, RateChains::Tx, now)
    }

    /// Starts setting both chains' sample rate, as
    /// [`Device::set_sample_rate`](crate::Device::set_sample_rate) does: the same machine,
    /// its waits as caller deadlines. [`poll`](Self::poll) reports
    /// [`NativeEvent::SampleRateSet`].
    ///
    /// # Errors
    ///
    /// Before any I/O: [`Error::Busy`] if an operation is pending; [`Error::NotPowered`]
    /// before a power-up; [`Error::AlreadyStreaming`] while a stream group exists;
    /// [`Error::UnsupportedSampleRate`] for a rate the board cannot run; [`Error::Bus`] if
    /// the handle is closed.
    pub fn start_sample_rate(
        &mut self,
        samples_per_second: u32,
        now: Instant,
    ) -> Result<(), Error> {
        self.start_rate(samples_per_second, RateChains::Both, now)
    }

    /// Starts a sample-rate change for `chains`.
    fn start_rate(&mut self, rate: u32, chains: RateChains, now: Instant) -> Result<(), Error> {
        let radio = self.refuse_radio_operation()?;
        if self.group.is_some() {
            return Err(Error::AlreadyStreaming);
        }
        let change =
            SampleRate::new(radio, rate, chains).ok_or(Error::UnsupportedSampleRate(rate))?;
        self.start_radio(RadioOp::Rate(Run::new(change)), RadioKind::SampleRate, now)
    }

    /// Starts tuning the receiver to `hz`, as
    /// [`Device::set_rx_frequency`](crate::Device::set_rx_frequency) does: the band, the
    /// PLL with its low-LO fallback, the NCOs and a filter that follows the rate.
    /// [`poll`](Self::poll) reports [`NativeEvent::Tuned`].
    ///
    /// # Errors
    ///
    /// Before any I/O: [`Error::Busy`] if an operation is pending; [`Error::NotPowered`]
    /// before a power-up; [`Error::Bus`] if the handle is closed. Then the tuning's
    /// failures, as the device's.
    pub fn start_rx_frequency(&mut self, hz: u32, now: Instant) -> Result<(), Error> {
        self.start_tune(Chain::Rx, hz, now)
    }

    /// Starts tuning the transmitter to `hz`, as
    /// [`Device::set_tx_frequency`](crate::Device::set_tx_frequency) does.
    /// [`poll`](Self::poll) reports [`NativeEvent::Tuned`].
    ///
    /// # Errors
    ///
    /// As [`start_rx_frequency`](Self::start_rx_frequency).
    pub fn start_tx_frequency(&mut self, hz: u32, now: Instant) -> Result<(), Error> {
        self.start_tune(Chain::Tx, hz, now)
    }

    /// Starts tuning both of `chain`'s NCO channels to `hz`.
    fn start_tune(&mut self, chain: Chain, hz: u32, now: Instant) -> Result<(), Error> {
        let radio = self.refuse_radio_operation()?;
        let tune = Tune::channel_frequency(radio, chain, NcoChannels::Both, hz);
        self.start_radio(RadioOp::Tune(Run::new(tune)), RadioKind::Tune, now)
    }

    /// Starts fixing the RX filter's bandwidth to `hz`, or with 0 returning it to following
    /// the sample rate, as [`Device::set_rx_bandwidth`](crate::Device::set_rx_bandwidth)
    /// does. [`poll`](Self::poll) reports [`NativeEvent::BandwidthSet`].
    ///
    /// # Errors
    ///
    /// As [`start_rx_frequency`](Self::start_rx_frequency).
    pub fn start_rx_bandwidth(&mut self, hz: u32, now: Instant) -> Result<(), Error> {
        let mut radio = self.refuse_radio_operation()?;
        radio.rx.fixed_bandwidth_hz = (hz != 0).then_some(hz);
        let script = radio.lms.set_rx_bandwidth(hz);
        self.start_script(radio, &script, RadioKind::Bandwidth, now)
    }

    /// Starts fixing the TX filter's bandwidth, as
    /// [`Device::set_tx_bandwidth`](crate::Device::set_tx_bandwidth) does.
    /// [`poll`](Self::poll) reports [`NativeEvent::BandwidthSet`].
    ///
    /// # Errors
    ///
    /// As [`start_rx_frequency`](Self::start_rx_frequency).
    pub fn start_tx_bandwidth(&mut self, hz: u32, now: Instant) -> Result<(), Error> {
        let mut radio = self.refuse_radio_operation()?;
        radio.tx.fixed_bandwidth_hz = (hz != 0).then_some(hz);
        let script = radio.lms.set_tx_bandwidth(hz);
        self.start_script(radio, &script, RadioKind::Bandwidth, now)
    }

    /// Starts setting an RX gain, as [`Device::set_rx_gain`](crate::Device::set_rx_gain)
    /// does. [`poll`](Self::poll) reports [`NativeEvent::GainSet`].
    ///
    /// # Errors
    ///
    /// As [`start_rx_frequency`](Self::start_rx_frequency).
    pub fn start_rx_gain(&mut self, gain: RxGain, now: Instant) -> Result<(), Error> {
        let mut radio = self.refuse_radio_operation()?;
        let script = radio.set_rx_gain(gain);
        self.start_script(radio, &script, RadioKind::Gain, now)
    }

    /// Starts setting a TX gain, as [`Device::set_tx_gain`](crate::Device::set_tx_gain)
    /// does. [`poll`](Self::poll) reports [`NativeEvent::GainSet`].
    ///
    /// # Errors
    ///
    /// As [`start_rx_frequency`](Self::start_rx_frequency).
    pub fn start_tx_gain(&mut self, gain: TxGain, now: Instant) -> Result<(), Error> {
        let mut radio = self.refuse_radio_operation()?;
        let script = radio.set_tx_gain(gain);
        self.start_script(radio, &script, RadioKind::Gain, now)
    }

    /// Starts selecting the RX path, as [`Device::set_rx_path`](crate::Device::set_rx_path)
    /// does. [`poll`](Self::poll) reports [`NativeEvent::PathSet`].
    ///
    /// # Errors
    ///
    /// As [`start_rx_frequency`](Self::start_rx_frequency).
    pub fn start_rx_path(&mut self, path: RxPath, now: Instant) -> Result<(), Error> {
        let radio = self.refuse_radio_operation()?;
        let select = Run::new(SetRxPath::new(radio, path));
        self.start_radio(RadioOp::RxPath(select), RadioKind::Path, now)
    }

    /// Starts selecting the TX path, as [`Device::set_tx_path`](crate::Device::set_tx_path)
    /// does. [`poll`](Self::poll) reports [`NativeEvent::PathSet`].
    ///
    /// # Errors
    ///
    /// As [`start_rx_frequency`](Self::start_rx_frequency).
    pub fn start_tx_path(&mut self, path: TxPath, now: Instant) -> Result<(), Error> {
        let mut radio = self.refuse_radio_operation()?;
        let script = radio.set_tx_path(path);
        self.start_script(radio, &script, RadioKind::Path, now)
    }

    /// Records `radio`, which `script` applies, as the blocking board records its state
    /// before writing it, and starts the writes.
    fn start_script(
        &mut self,
        radio: Radio,
        script: &Script,
        kind: RadioKind,
        now: Instant,
    ) -> Result<(), Error> {
        self.session.board.radio = Some(radio);
        self.start_radio(RadioOp::Script(*script), kind, now)
    }

    /// A stream group's RF step for `chain`: its create's (`up`) or its stop's, from the
    /// handle's RF state now; `None` on a handle without one, whose group configures only
    /// the stream engine.
    pub(super) fn group_radio_step(&self, chain: Chain, up: bool) -> Option<SequenceOperation> {
        let radio = self.session.board.radio?;
        let op = if up {
            RadioOp::Up(Run::new(StreamUp::new(radio, chain)))
        } else {
            RadioOp::Power(stream_down(radio, chain))
        };
        Some(SequenceOperation::Radio(RadioRun::new(
            op,
            RadioKind::GroupStep,
        )))
    }
}
