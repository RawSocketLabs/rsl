//! Compile witness for the exact private production core, not another driver implementation.
//! A bare-metal check proves core + alloc compilation, not linking or hardware operation.
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

#[path = "../../src/portable/mod.rs"]
mod portable;

use core::convert::Infallible;
use core::time::Duration;

use bnb::{u4, u12};
use portable::address::{I2C_MAX_READ, I2C_MAX_WRITE, I2cAddr, SpiAddr};
use portable::clock::Layout;
use portable::error::Failure;
use portable::lp8758::{BuckControl, BuckVoltage, Config};
use portable::power::{PowerRails, RailPlan};
use portable::sequence::{FpgaAccess, RegisterAccess, Sequence, SequenceAction, SequenceFailure};
use portable::si5332::{ClockSequence, LvpeclOutput, Reference};
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
pub fn fpga_read(reg: u16, readback: &[u8]) -> Result<u32, &'static str> {
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
pub fn fpga_write(reg: u16, value: u32) -> Result<u32, &'static str> {
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
                SequenceFailure::ClockInputMissing => "no input clock",
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
                SequenceFailure::ClockInputMissing => "no input clock",
                SequenceFailure::ChipId { chip, .. }
                | SequenceFailure::Transport { chip, .. }
                | SequenceFailure::FpgaTransport { chip, .. } => chip,
            })?;
        actions += 1;
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
    fn the_exact_source_witness_runs_every_fpga_action_through_the_register_machine() {
        // HWID readback: raw little-endian words with no length header (PIN 3670f15).
        let hwid = [0x00, 0x03, 0x00, 0x42];
        assert_eq!(fpga_read(19, &hwid), Ok(0x4200_0300));
        assert_eq!(
            fpga_read(19, &hwid[..2]),
            Err("USB register readback length differs from request")
        );
        assert_eq!(fpga_write(0, 1), Ok(0));
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
