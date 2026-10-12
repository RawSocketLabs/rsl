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
use portable::temperature::{DeviceId, Reg, TemperatureRead};
use portable::usb::i2c::{Action, Event, I2cTransaction};
use portable::usb::protocol::{Command, I2cCache};

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

#[cfg(test)]
mod tests {
    use super::*;

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
