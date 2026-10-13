//! Compile witness for the exact private production core, not another driver implementation.
//! A bare-metal check proves core + alloc compilation, not linking or hardware operation.
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

#[path = "../../src/portable/mod.rs"]
mod portable;

use core::convert::Infallible;
use core::time::Duration;

use bnb::{u4, u7, u12};
use portable::address::{I2C_MAX_READ, I2C_MAX_WRITE, I2cAddr, SpiAddr};
use portable::clock::Layout;
use portable::error::Failure;
use portable::fpga::{Gpo, Phy, SubReg};
use portable::lms6002d::{Lms6002d, RxDcCalibration, spi};
use portable::lp8758::{BuckControl, BuckVoltage, Config};
use portable::power::{PowerRails, RailPlan};
use portable::radio::{
    Chain, ChainPower, Chains, NcoChannels, OwnsRadio, PowerUp, PowerUpPlan, Radio, RateChains,
    Run, RxGain, RxPath, SampleRate, SetRxPath, Steps, StreamUp, Tune, TxGain, TxPath, power_down,
    stream_down, wiring,
};
use portable::sequence::{FpgaAccess, RegisterAccess, Sequence, SequenceAction, SequenceFailure};
use portable::si5332::{ClockSequence, LvpeclOutput, Reference};
use portable::stream::{EngineSequence, RxFormat, SyncTrigger, TxLanes};
use portable::temperature::{DeviceId, Reg, TemperatureRead};
use portable::tps6381x::OutputVoltage;
use portable::usb::i2c::{Action, Event, I2cTransaction};
use portable::usb::protocol::{Command, I2cCache};
use portable::usb::register::{self as register_usb, RegisterTransaction};

/// Exercises real chip register macro expansions, including checked enum conversion.
#[must_use]
pub fn sensor_identity(raw: u16) -> bool {
    let expected = DeviceId::new()
        .with_revision(u4::new(1))
        .with_device(u12::new(0x114));
    let found = DeviceId::from_raw(raw);
    Reg::try_from(0x0b) == Ok(Reg::DeviceId) && found == expected
}

/// Exercises the production semantic identities and FPGA transfer bounds.
#[must_use]
pub fn bus_identities() -> (u8, u8, usize, usize) {
    let sensor = I2cAddr::new(0, 0x4e);
    (
        sensor.bus(),
        SpiAddr(0x0001_0002).bus(),
        I2C_MAX_WRITE,
        I2C_MAX_READ,
    )
}

/// Decodes a real sensor result without host errors, I/O or allocation in the operation.
#[must_use]
pub fn temperature(bytes: &[u8]) -> Option<f32> {
    let (mut operation, request) = TemperatureRead::begin(I2cAddr::new(0, 0x4e));
    if request.dev.addr() != 0x4e || request.register != [0] {
        return None;
    }
    operation.complete::<Infallible>(Ok(bytes))?.ok()
}

/// Reads every field of the real scalar clock plan, including its VCO accessor.
#[must_use]
pub fn clock(reference_hz: u32, out_hz: u32) -> Option<([u32; 9], bool)> {
    let plan = Layout::new(reference_hz, out_hz)?;
    Some((
        [
            plan.out_hz,
            plan.vco_hz(),
            plan.hs_divider,
            plan.output_divider,
            plan.prescaler,
            plan.pll_input_hz,
            plan.integer,
            plan.residue,
            plan.denominator,
        ],
        plan.from_reference,
    ))
}

/// Exercises the actual command bitfield macro for a bounded physical register request.
#[must_use]
pub fn register_command(address: u16, read: bool) -> [u8; 4] {
    Command::encode(address, 64, read)
}

/// Runs the exact temperature and USB machines with an externally supplied IRQ and time.
#[must_use]
pub fn usb_temperature(packet: &[u8], elapsed: Duration) -> Option<f32> {
    let (mut temperature, request) = TemperatureRead::begin(I2cAddr::new(0, 0x4e));
    let mut transaction = I2cTransaction::<Infallible>::new(
        7,
        request.dev,
        &request.register,
        2,
        &mut I2cCache::default(),
        Duration::ZERO,
    )
    .ok()?;
    let write = transaction.start(Duration::ZERO).ok()?;
    if !matches!(
        transaction.dispatch(write, Duration::ZERO).ok()?,
        Action::Write { .. }
    ) {
        return None;
    }
    let mut sequence = None;
    let notification = transaction
        .complete(Event::Written { id: 7 }, Duration::ZERO, &mut sequence)
        .ok()?;
    transaction.advance(elapsed).ok()?;
    transaction.dispatch(notification, elapsed).ok()?;
    let done = transaction
        .complete(
            Event::Notification { id: 7, packet },
            elapsed,
            &mut sequence,
        )
        .ok()?;
    let Action::Complete { bytes, len } = transaction.dispatch(done, elapsed).ok()? else {
        return None;
    };
    temperature.complete::<Infallible>(Ok(&bytes[..len]))?.ok()
}

/// Proves a caller's transport error needs no std error trait or heap ownership.
#[must_use]
pub fn transport_error_is_preserved(error: u8) -> bool {
    let mut transaction = I2cTransaction::<u8>::new(
        8,
        I2cAddr::new(0, 0x4e),
        &[0],
        2,
        &mut I2cCache::default(),
        Duration::ZERO,
    )
    .expect("invariant: bounded sensor request and clock");
    transaction
        .start(Duration::ZERO)
        .expect("invariant: fresh transaction");
    matches!(transaction.complete(Event::Failed { id: 8, error }, Duration::ZERO, &mut None),
        Err(Failure::Transport(found)) if found == error)
}

/// Reads gateware register `reg` through the real sequence action and register machine,
/// answered by the whole `readback` transfer.
///
/// # Errors
///
/// As [`fpga_action`].
pub fn fpga_read(reg: u32, readback: &[u8]) -> Result<u32, &'static str> {
    fpga_action(
        SequenceAction::Fpga(FpgaAccess::Read { reg }),
        readback,
        &[],
    )
}

/// Writes gateware register `reg` through the real sequence action and register machine.
///
/// # Errors
///
/// As [`fpga_action`].
pub fn fpga_write(reg: u32, value: u32) -> Result<u32, &'static str> {
    fpga_action(
        SequenceAction::Fpga(FpgaAccess::Write { reg, value }),
        &[],
        &[],
    )
}

/// Shifts `word` through SPI bus `bus` through the real sequence action and register machine,
/// answered by the whole IRQ transfer `notification`.
///
/// # Errors
///
/// As [`fpga_action`].
pub fn spi_exchange(bus: u32, word: u32, notification: &[u8]) -> Result<u32, &'static str> {
    let access = FpgaAccess::Spi {
        target: SpiAddr(bus),
        word,
        reg: 0,
        write: true,
    };
    fpga_action(SequenceAction::Fpga(access), &[], notification)
}

/// Runs one FPGA sequence action end to end through the real register/SPI machine, answering
/// a readback or IRQ wait with the given whole transfers; returns the word it reports (zero
/// for a write).
///
/// # Errors
///
/// The machine's refusal or failure reason, `"timeout"`, or `"not an FPGA action"`.
fn fpga_action(
    action: SequenceAction,
    readback: &[u8],
    notification: &[u8],
) -> Result<u32, &'static str> {
    let SequenceAction::Fpga(access) = action else {
        return Err("not an FPGA action");
    };
    let mut transaction =
        RegisterTransaction::<Infallible>::access(3, access, Duration::ZERO).map_err(reason)?;
    let mut sequence = None;
    let mut action = transaction.start(Duration::ZERO).map_err(reason)?;
    loop {
        action = transaction
            .dispatch(action, Duration::ZERO)
            .map_err(reason)?;
        let event = match action {
            register_usb::Action::Write { id, .. } => Event::Written { id },
            register_usb::Action::Readback { id, .. } => Event::Readback {
                id,
                packet: readback,
            },
            register_usb::Action::Notification { id, .. } => Event::Notification {
                id,
                packet: notification,
            },
            register_usb::Action::Complete { words, len } => {
                return Ok(words[..len].first().copied().unwrap_or(0));
            }
        };
        action = transaction
            .complete(event, Duration::ZERO, &mut sequence)
            .map_err(reason)?;
    }
}

/// The command frame length of a real multi-word register write, or `None` when refused.
#[must_use]
pub fn register_write_frame_len(values: &[u32]) -> Option<usize> {
    let mut transaction =
        RegisterTransaction::<Infallible>::write(4, 0x10, values, Duration::ZERO).ok()?;
    match transaction.start(Duration::ZERO).ok()? {
        register_usb::Action::Write { frame, .. } => Some(frame.as_bytes().len()),
        _ => None,
    }
}

/// A portable failure as the witness reports it; `Infallible` transport errors cannot occur.
fn reason(failure: Failure<Infallible>) -> &'static str {
    match failure {
        Failure::Timeout => "timeout",
        Failure::Unsupported(reason) | Failure::InvalidData(reason) => reason,
        Failure::Transport(never) => match never {},
    }
}

/// Runs the real power-rail machine with the board's setpoints, answering its reads from
/// `reads`; returns the accesses completed, or the name of the chip that failed.
///
/// # Errors
///
/// The failing chip's name, or `"missing read"` when `reads` runs out.
pub fn power_rails(reads: &[u8]) -> Result<usize, &'static str> {
    let mut rails = PowerRails::new(RailPlan {
        pmic: I2cAddr::new(0, 0x60),
        config: Config::new().with_warn_at_105c(true),
        gpio: BuckVoltage::from_millivolts(1800),
        lms_io: BuckVoltage::from_millivolts(1800),
        control: BuckControl::new()
            .with_enabled(true)
            .with_discharge_when_off(true)
            .with_forced_pwm(true),
        boost: I2cAddr::new(0, 0x75),
        boost_forced_pwm: true,
        boost_voltage: OutputVoltage::from_millivolts(3450),
    });
    let mut reads = reads.iter().copied();
    let mut completed = 0;
    while let Some(access) = rails.access() {
        let byte = match access {
            RegisterAccess::Read { .. } => reads.next().ok_or("missing read")?,
            RegisterAccess::Write { value, .. } => value,
        };
        rails
            .complete::<Infallible>(Ok(byte))
            .map_err(|failure| match failure {
                SequenceFailure::ChipId { chip, .. }
                | SequenceFailure::Transport { chip, .. }
                | SequenceFailure::FpgaTransport { chip, .. }
                | SequenceFailure::ChipMissing { chip } => chip,
                SequenceFailure::InputMissing { .. } => "no input clock",
            })?;
        completed += 1;
    }
    Ok(completed)
}

/// Runs the real Si5332 power-up program and state wait for a revision 3 board (on-board
/// oscillator, output 0 LVPECL) or an earlier one, answering identification reads with
/// `id` and state polls with `state`; returns the actions taken.
///
/// # Errors
///
/// `"missing"`, `"no input clock"` or the failing chip's name.
pub fn clock_power_up(rev3: bool, id: u8, state: u8) -> Result<usize, &'static str> {
    let dev = I2cAddr::new(0, 0x6a);
    let (reference, lvpecl) = if rev3 {
        (Reference::Oscillator, LvpeclOutput::Out0)
    } else {
        (Reference::Input2, LvpeclOutput::Out1)
    };
    let mut program = ClockSequence::program(dev, 1, reference, lvpecl);
    let mut actions = drive(&mut program, id, state)?;
    actions += drive(&mut ClockSequence::wait_active(dev), id, state)?;
    Ok(actions)
}

/// Runs the real Si5332 sample-clock plan for `out_hz` from the 26 MHz reference.
///
/// # Errors
///
/// `"no plan"` for an unreachable rate, else as [`clock_power_up`].
pub fn clock_layout(out_hz: u32, state: u8) -> Result<usize, &'static str> {
    let layout = Layout::new(26_000_000, out_hz).ok_or("no plan")?;
    let dev = I2cAddr::new(0, 0x6a);
    drive(
        &mut ClockSequence::layout(dev, &layout, LvpeclOutput::Out0, 8),
        0,
        state,
    )
}

/// Runs the real Si5332 reference switch to clock input 2 (`external`) or the oscillator.
///
/// # Errors
///
/// As [`clock_power_up`].
pub fn clock_reference_switch(external: bool, state: u8) -> Result<usize, &'static str> {
    let reference = if external {
        Reference::Input2
    } else {
        Reference::Oscillator
    };
    let dev = I2cAddr::new(0, 0x6a);
    drive(
        &mut ClockSequence::reference_switch(dev, reference),
        0,
        state,
    )
}

/// Runs the real stream-engine setup for one 1024-sample `ci16` burst, then a duplex start
/// and the RX and TX stops, answering the DMA status read with `status`; returns the
/// register accesses made.
///
/// # Errors
///
/// The failing engine's name, `"invalid format"` or `"unexpected access"`.
pub fn stream_engine(status: u32) -> Result<usize, &'static str> {
    let format = RxFormat::new(1024, 512, 1, 0x0040_3fe7).ok_or("invalid format")?;
    let mut accesses = 0;
    for sequence in [
        EngineSequence::rx_reset(),
        EngineSequence::rx_configure(&format),
        EngineSequence::start(true, Some(2), SyncTrigger::Duplex),
        EngineSequence::new().run_rx(false),
        EngineSequence::new().stop_tx(),
    ] {
        accesses += engine(sequence, status)?;
    }
    Ok(accesses)
}

/// Runs the real stream controls: a sync write for every trigger, the throttle on and off,
/// the host-ready signal, a TX mute, a channel-1 lane update and a TX create's five DSP-port
/// writes; returns the register accesses made.
///
/// # Errors
///
/// As [`stream_engine`].
pub fn stream_controls() -> Result<usize, &'static str> {
    let triggers = [
        SyncTrigger::Stop,
        SyncTrigger::OnePps,
        SyncTrigger::RxActivity,
        SyncTrigger::TxActivity,
        SyncTrigger::EitherActivity,
        SyncTrigger::Duplex,
        SyncTrigger::FreeRun,
        SyncTrigger::GeneratedSysref,
    ];
    let mut controls = EngineSequence::new()
        .throttle_rx(Some((1, 2)))
        .throttle_rx(None)
        .signal_rx_ready()
        .update_tx_flags(3)
        .update_tx_lanes(TxLanes {
            mute: 1,
            swap: true,
            channels_log2: 0,
        });
    let mut accesses = engine(controls, 0)?;
    for trigger in triggers {
        controls = EngineSequence::new().sync(trigger);
        accesses += engine(controls, 0)?;
    }
    let ports = EngineSequence::port_writes(57, &[0x0700_0000, 1, 2, 3, 4]);
    accesses += engine(ports, 0)?;
    Ok(accesses)
}

/// Drives one engine sequence to its end, answering reads with `status`.
fn engine(mut sequence: EngineSequence, status: u32) -> Result<usize, &'static str> {
    let mut accesses = 0;
    while let Some(action) = sequence.next() {
        let word = match action {
            SequenceAction::Fpga(FpgaAccess::Read { .. }) => status,
            SequenceAction::Fpga(FpgaAccess::Write { value, .. }) => value,
            _ => return Err("unexpected access"),
        };
        sequence
            .complete::<Infallible>(Ok(word))
            .map_err(|failure| match failure {
                SequenceFailure::ChipMissing { chip }
                | SequenceFailure::ChipId { chip, .. }
                | SequenceFailure::Transport { chip, .. }
                | SequenceFailure::FpgaTransport { chip, .. }
                | SequenceFailure::InputMissing { chip } => chip,
            })?;
        accesses += 1;
    }
    Ok(accesses)
}

/// Drives one sequence to its end with scripted reads; waits pass at once.
fn drive(sequence: &mut impl Sequence, id: u8, state: u8) -> Result<usize, &'static str> {
    let mut actions = 0;
    while let Some(action) = sequence.next() {
        let byte = match action {
            SequenceAction::Access(RegisterAccess::Read { reg: 0x07, .. }) => state,
            SequenceAction::Access(RegisterAccess::Read { .. }) => id,
            SequenceAction::Access(RegisterAccess::Write { value, .. }) => value,
            SequenceAction::Sleep(_) => 0,
            SequenceAction::Fpga(_) => return Err("unexpected FPGA access"),
        };
        sequence
            .complete::<Infallible>(Ok(byte.into()))
            .map_err(|failure| match failure {
                SequenceFailure::ChipMissing { .. } => "missing",
                SequenceFailure::InputMissing { .. } => "no input clock",
                SequenceFailure::ChipId { chip, .. }
                | SequenceFailure::Transport { chip, .. }
                | SequenceFailure::FpgaTransport { chip, .. } => chip,
            })?;
        actions += 1;
    }
    Ok(actions)
}

/// A board answering the RF sequences without I/O: the LMS6002D's registers read back what
/// was last written to them, the PMIC, boost converter and Si5332 identify as the uSDR's
/// parts and the Si5332 reports ACTIVE; gateware reads return 0.
struct RfBoard {
    /// The LMS6002D's 128 registers.
    lms: [u8; 128],
}

impl RfBoard {
    /// A board whose registers all read 0.
    const fn new() -> Self {
        Self { lms: [0; 128] }
    }

    /// The word `action` reports.
    fn answer(&mut self, action: SequenceAction) -> u32 {
        match action {
            SequenceAction::Fpga(FpgaAccess::Spi { word, write, .. }) => {
                let [data, addr, ..] = word.to_le_bytes();
                let reg = usize::from(addr & 0x7f);
                if write {
                    self.lms[reg] = data;
                }
                self.lms[reg].into()
            }
            SequenceAction::Access(RegisterAccess::Read { dev, reg }) => match (dev.addr(), reg) {
                (0x60, 0x00) => 0x01,
                (0x60, 0x01) => 0xe0,
                (0x75, 0x03) => 0x04,
                (0x6a, 0x07) => 0x02,
                _ => 0,
            },
            SequenceAction::Fpga(_)
            | SequenceAction::Access(RegisterAccess::Write { .. })
            | SequenceAction::Sleep(_) => 0,
        }
    }

    /// Runs `sequence` to its end, actions after a failure included; the actions taken and
    /// the first failure's chip, if any.
    fn run(&mut self, sequence: &mut impl Sequence) -> (usize, Option<&'static str>) {
        let (mut actions, mut failure) = (0, None);
        while let Some(action) = sequence.next() {
            actions += 1;
            let word = self.answer(action);
            if let Err(found) = sequence.complete::<Infallible>(Ok(word)) {
                failure.get_or_insert(chip_of(&found));
                if !sequence.attempts_after_failure() {
                    break;
                }
            }
        }
        (actions, failure)
    }
}

/// The chip a failure names.
fn chip_of(failure: &SequenceFailure<Infallible>) -> &'static str {
    match failure {
        SequenceFailure::ChipMissing { chip }
        | SequenceFailure::ChipId { chip, .. }
        | SequenceFailure::Transport { chip, .. }
        | SequenceFailure::FpgaTransport { chip, .. }
        | SequenceFailure::InputMissing { chip } => chip,
    }
}

/// Runs a composite RF operation over `board`, giving the state back: the actions taken,
/// and the first failure's chip or the fault that ended it.
fn rf<S: Steps + OwnsRadio>(
    board: &mut RfBoard,
    steps: S,
) -> (Radio, usize, Result<(), &'static str>) {
    let mut run = Run::new(steps);
    let (actions, failure) = board.run(&mut run);
    let outcome = match (run.fault(), failure) {
        (Some(_), _) => Err("tuning fault"),
        (None, Some(chip)) => Err(chip),
        (None, None) => Ok(()),
    };
    (run.radio(), actions, outcome)
}

/// Powers a revision-3 board up through the real power-up machine.
///
/// # Errors
///
/// The failing chip's name.
pub fn rf_power_up() -> Result<usize, &'static str> {
    let mut board = RfBoard::new();
    let (_, actions) = power_up(&mut board)?;
    Ok(actions)
}

/// The real power-up over `board`: the powered radio and the actions taken.
fn power_up(board: &mut RfBoard) -> Result<(Radio, usize), &'static str> {
    let plan = PowerUpPlan {
        rails: RailPlan {
            pmic: wiring::PMIC,
            config: Config::new().with_warn_at_105c(true),
            gpio: BuckVoltage::from_millivolts(1800),
            lms_io: wiring::LMS_VIO_NORMAL,
            control: BuckControl::new()
                .with_enabled(true)
                .with_discharge_when_off(true)
                .with_forced_pwm(true),
            boost: I2cAddr::new(0, 0x75),
            boost_forced_pwm: true,
            boost_voltage: OutputVoltage::from_millivolts(3450),
        },
        clock: wiring::CLOCK,
        reference: Reference::Oscillator,
        lvpecl: wiring::lvpecl(true),
        rev3: true,
        oscillator: true,
        chains: Chains {
            rx: true,
            tx: true,
            extended_tx_core: false,
        },
        reference_hz: 26_000_000,
    };
    let mut run = Run::new(PowerUp::new(plan));
    let (actions, failure) = board.run(&mut run);
    if let Some(chip) = failure {
        return Err(chip);
    }
    let radio = run.steps().radio().ok_or("not configured")?;
    Ok((radio, actions))
}

/// Powers a board up, then sets a sample rate, tunes both chains (the RX low band through
/// the mixer), sets gains and paths and creates and destroys both streams' RF parts, all
/// through the real machines; the actions each took.
///
/// # Errors
///
/// The failing step's chip, or `"tuning fault"`.
pub fn rf_set_up(rate: u32, rx_hz: u32) -> Result<[usize; 8], &'static str> {
    let mut board = RfBoard::new();
    let (radio, _) = power_up(&mut board)?;
    let rate = SampleRate::new(radio, rate, RateChains::Both).ok_or("no rate")?;
    let (factor, adc) = (rate.factor(), rate.adc_rate_hz());
    let (mut radio, rated, ok) = rf(&mut board, rate);
    ok?;
    if adc != factor.factor() * (adc / factor.factor()) {
        return Err("inconsistent rate");
    }
    let (next, rx, ok) = rf(
        &mut board,
        Tune::channel_frequency(radio, Chain::Rx, NcoChannels::Both, rx_hz),
    );
    ok?;
    let (next, tx, ok) = rf(
        &mut board,
        Tune::channel_frequency(next, Chain::Tx, NcoChannels::Nco0, 1_200_000_000),
    );
    ok?;
    let (next, offset, ok) = rf(
        &mut board,
        Tune::baseband_offset(next, Chain::Tx, NcoChannels::Nco1, 5_000),
    );
    ok?;
    radio = next;
    let mut gains = radio
        .set_rx_gain(RxGain::Auto(40))
        .extend(&radio.set_rx_gain(RxGain::Lna(4)))
        .extend(&radio.set_rx_gain(RxGain::Vga1(20)))
        .extend(&radio.set_rx_gain(RxGain::Vga2(9)))
        .extend(&radio.set_rx_gain(RxGain::Vga2a(6)))
        .extend(&radio.set_rx_gain(RxGain::Vga2b(3)))
        .extend(&radio.set_tx_gain(TxGain::Vga1(-10)))
        .extend(&radio.set_tx_gain(TxGain::Vga2(10)))
        .extend(&radio.set_tx_path(TxPath::High));
    let (paths, failure) = board.run(&mut gains);
    if let Some(chip) = failure {
        return Err(chip);
    }
    let (next, rx_path, ok) = rf(&mut board, SetRxPath::new(radio, RxPath::WideLoopback));
    ok?;
    let (next, ..) = rf(&mut board, SetRxPath::new(next, RxPath::Auto));
    let (next, up, ok) = rf(&mut board, StreamUp::new(next, Chain::Rx));
    ok?;
    let (next, _, ok) = rf(&mut board, StreamUp::new(next, Chain::Tx));
    ok?;
    let mut down = stream_down(next, Chain::Rx);
    let (downs, _) = board.run(&mut down);
    let mut down = stream_down(down.radio(), Chain::Tx);
    board.run(&mut down);
    let mut off = power_down();
    board.run(&mut off);
    let mut power = Run::new(ChainPower::new(down.radio(), Chain::Rx, true));
    board.run(&mut power);
    Ok([rated, rx, tx, offset, paths, rx_path, up, downs])
}

/// Builds each LMS6002D control the board's calibration and debug paths use and counts
/// their accesses.
#[must_use]
pub fn lms_controls() -> usize {
    let (mut lms, _) = Lms6002d::create(Lms6002d::USDR_TARGET);
    let scripts = [
        Lms6002d::read_id(Lms6002d::USDR_TARGET),
        lms.read_register_raw(u7::new(0x04)),
        lms.write_register_raw(u7::new(0x04), 1),
        lms.set_tx_vga1_code(lms.tx_vga1_code()),
        lms.set_tx_vga1_dc(0x80, 0x80),
        lms.disable_rx_pll(),
        lms.disable_tx_pll(),
        lms.set_rx_dc_trim(u7::new(1), u7::new(2)),
        lms.set_rx_ip2_trim(u7::new(3), u7::new(4)),
        lms.enable_rf_loopback(),
        lms.disable_rf_loopback(),
        lms.select_lna(lms.active_lna()),
    ];
    let mut board = RfBoard::new();
    let mut total = 0;
    for mut script in scripts {
        total += board.run(&mut script).0;
    }
    let mut calibration = RxDcCalibration::new(lms, 26_000_000, (1, 2));
    total += board.run(&mut calibration).0;
    total + usize::from(calibration.fault().is_some()) + usize::from(spi::value(0x1ff) == 0xff)
}

/// The words of every GPO bank's latch and every DSP-port sub-register write the driver
/// makes, folded together.
#[must_use]
pub fn gateware_words() -> u32 {
    let banks = [
        Gpo::LmsReset,
        Gpo::RxMixerEnable,
        Gpo::TxSwitch,
        Gpo::RxSwitch,
        Gpo::Booster,
        Gpo::Led,
        Gpo::DcCorrection,
        Gpo::ClockMeasurement,
        Gpo::EnableOscillator,
    ];
    let subs = [
        SubReg::Reset,
        SubReg::DcControl,
        SubReg::IqAmplitudeI,
        SubReg::IqAmplitudeQ,
        SubReg::IqPhaseI,
        SubReg::IqPhaseQ,
        SubReg::Generator,
        SubReg::Nco0Low,
        SubReg::Nco0High,
        SubReg::Nco1Low,
        SubReg::Nco1High,
        SubReg::FirLoad,
    ];
    let word = |access: FpgaAccess| match access {
        FpgaAccess::Write { value, .. } => value,
        _ => 0,
    };
    let gpo = banks.iter().fold(0, |all, bank| all ^ word(bank.write(1)));
    subs.iter()
        .fold(gpo, |all, &sub| all ^ word(Phy::Tx.write(sub, 1)))
}

/// Selects every RX and TX path and sets a rate for each chain choice on a powered board,
/// through the real machines; the actions taken.
///
/// # Errors
///
/// The failing chip's name, or `"no rate"`.
pub fn rf_paths() -> Result<usize, &'static str> {
    let mut board = RfBoard::new();
    let (mut radio, mut actions) = power_up(&mut board)?;
    for chains in [RateChains::Rx, RateChains::Tx, RateChains::Both] {
        let rate = SampleRate::new(radio, 40_000_000, chains).ok_or("no rate")?;
        let (next, taken, ok) = rf(&mut board, rate);
        ok?;
        (radio, actions) = (next, actions + taken);
    }
    for path in [
        RxPath::Low,
        RxPath::Wide,
        RxPath::High,
        RxPath::LowLoopback,
        RxPath::WideLoopback,
        RxPath::HighLoopback,
        RxPath::Auto,
    ] {
        let (next, taken, ok) = rf(&mut board, SetRxPath::new(radio, path));
        ok?;
        (radio, actions) = (next, actions + taken);
    }
    for path in [TxPath::Wide, TxPath::High, TxPath::Auto] {
        let mut script = radio.set_tx_path(path);
        actions += board.run(&mut script).0;
    }
    Ok(actions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_exact_source_witness_runs_the_pinned_clock_sequences() {
        // Nine identification reads, the supply read, one wait and poll, 49 writes; then
        // one wait and poll.
        assert_eq!(clock_power_up(true, 0x00, 0x02), Ok(63));
        assert_eq!(clock_power_up(false, 0x00, 0x02), Ok(63));
        assert_eq!(clock_power_up(true, 0xff, 0x02), Err("missing"));
        assert_eq!(clock_power_up(true, 0x00, 0x89), Err("no input clock"));
        // A wait and poll, 25 writes, then another wait and poll.
        assert_eq!(clock_layout(20_000_000, 0x02), Ok(29));
        // A wait and poll, five writes, then another wait and poll.
        assert_eq!(clock_reference_switch(true, 0x02), Ok(9));
        assert_eq!(clock_reference_switch(false, 0x89), Err("no input clock"));
    }

    #[test]
    fn the_exact_source_witness_runs_the_stream_engine_sequences() {
        // Setup: status, two DMA writes, four front-end commands, two DMA sizes, the sync;
        // start: RX run (two), TX start (three), the sync; RX halt (two); TX stop (one).
        assert_eq!(stream_engine(0), Ok(10 + 6 + 2 + 1));
        assert_eq!(stream_engine(0xffff_ffff), Err("RX DMA engine"));
        assert_eq!(stream_controls(), Ok(5 + 8 + 5));
    }

    #[test]
    fn the_exact_source_witness_runs_every_fpga_action_through_the_register_machine() {
        // HWID readback: raw little-endian words with no length header (PIN 3670f15).
        let hwid = [0x00, 0x03, 0x00, 0x42];
        assert_eq!(fpga_read(19, &hwid), Ok(0x4200_0300));
        assert_eq!(
            fpga_read(19, &hwid[..2]),
            Err("USB register readback length differs from request")
        );
        assert_eq!(fpga_write(0, 1), Ok(0));
        assert_eq!(
            fpga_write(0x1_0100, 1),
            Err("USB register commands address 16-bit physical registers"),
            "the stream engine's configuration space is reached through the index window"
        );
        // Scalar IRQ 2, sequence 0, SPI reply 0xabcd.
        let reply = [2, 0, 0, 0, 0xcd, 0xab, 0, 0];
        assert_eq!(spi_exchange(0, 0x8000_0001, &reply), Ok(0xabcd));
        assert_eq!(
            spi_exchange(1, 0x8000_0001, &reply),
            Err("uSDR has only simple SPI bus 0")
        );
        assert_eq!(
            fpga_action(SequenceAction::Sleep(Duration::ZERO), &[], &[]),
            Err("not an FPGA action")
        );
        assert_eq!(register_write_frame_len(&[0; 63]), Some(256));
        assert_eq!(register_write_frame_len(&[0; 64]), None);
    }

    #[test]
    fn the_exact_source_witness_runs_the_pinned_power_rail_sequence() {
        // Two PMIC revision reads, its configuration, two voltages and four channel
        // controls, then the boost converter's ID read, control and two voltages.
        assert_eq!(power_rails(&[0x01, 0xe0, 0x04]), Ok(13));
        assert_eq!(power_rails(&[0x01, 0xe1]), Err("LP8758"));
    }

    #[test]
    fn the_exact_source_witness_executes_the_pinned_temperature_transaction() {
        // PIN3670f15: scalar IRQ6, sequence0, TMP114 signed wire counts0x0c80 (25°C).
        let packet = [6, 0, 0, 0, 0x0c, 0x80, 0, 0];
        assert_eq!(usb_temperature(&packet, Duration::ZERO), Some(25.0));
        assert_eq!(usb_temperature(&packet, Duration::from_secs(1)), None);
        assert_eq!(usb_temperature(&packet[..4], Duration::ZERO), None);
        assert!(sensor_identity(0x1114));
        assert!(!sensor_identity(0xffff));
        assert!(transport_error_is_preserved(42));
    }

    #[test]
    fn the_exact_source_witness_runs_the_board_rf_machines() {
        assert!(rf_power_up().is_ok_and(|actions| actions > 0));
        // A sample rate, an RX tune in the mixer band, both TX tunes, the gains and paths,
        // and both streams' RF parts: every step reaches the board and none fails.
        let actions = rf_set_up(20_000_000, 100_000_000).unwrap();
        assert!(actions.iter().all(|&taken| taken > 0), "{actions:?}");
        assert!(lms_controls() > 0);
        assert!(rf_paths().is_ok_and(|actions| actions > 0));
        assert_ne!(gateware_words(), 0);
    }

    #[test]
    fn cancellation_still_requires_executor_cleanup_once_issued() {
        let mut transaction = I2cTransaction::<u8>::new(
            7,
            I2cAddr::new(0, 0x4e),
            &[0],
            2,
            &mut I2cCache::default(),
            Duration::ZERO,
        )
        .unwrap();
        transaction.start(Duration::ZERO).unwrap();
        assert!(matches!(
            transaction.cancel(Duration::ZERO),
            Err(Failure::Unsupported(_))
        ));
        assert!(
            transaction
                .complete(Event::Written { id: 7 }, Duration::ZERO, &mut None)
                .is_err()
        );
    }
}
