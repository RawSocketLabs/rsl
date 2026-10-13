//! TMP114 temperature sensor, which reads the board temperature for the thermal policy.
//!
//! # What it does
//!
//! The TMP114 is a small digital thermometer. It converts on its own, continuously, and
//! holds the latest reading in a result register. A read returns that last result, so it
//! never waits on a conversion. Until the first conversion completes the result reads
//! 0 °C, which looks cool; the driver assumes one has completed by the time it reads.
//! It measures the board where it is mounted, not the die of
//! any other chip: the FPGA or LMS6002D can be hotter than it reports.
//!
//! The TMP114 comes in variants with different fixed I2C addresses; the uSDR
//! carries the TMP114NB, at 0x4E.
//!
//! # How the driver uses it
//!
//! It is the first chip the driver talks to after reading the board revision.
//! [`Tmp114::check_id`] confirms a TMP114 answers, because a missing sensor would read as
//! −0.0078 °C (0xFFFF) and look cool. [`Tmp114::celsius`] then feeds the thermal policy,
//! before anything else is powered and again whenever the caller asks.
//!
//! # Sources
//!
//! Register meanings: TI SNIS214E, "TMP114", §8.6. Its registers are 16 bits wide and sent
//! MSB first.

use bnb::{u4, u12};

use crate::error::{BusContext, Error};
use crate::lowlevel::{Bus, BusError, I2cAddr};
use crate::portable::temperature::{DeviceId, Reg, TemperatureRead as Operation};

pub(crate) use crate::portable::temperature::TemperatureRequest;

/// A TMP114 on the I2C bus.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tmp114 {
    /// Where the sensor answers.
    dev: I2cAddr,
}

impl Tmp114 {
    /// The chip's name in errors.
    const NAME: &'static str = "TMP114";

    /// The ID libusdr requires: device 0x114 (TMP114), revision 1 (0x1114). An idle bus reads
    /// 0xFFFF.
    const DEVICE_ID: DeviceId = DeviceId::new()
        .with_revision(u4::new(1))
        .with_device(u12::new(0x114));

    /// The sensor at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self { dev }
    }

    /// The sensor where the uSDR wires it: FPGA I2C bus 0, address 0x4E, the TMP114NB variant's
    /// fixed address (`I2C_DEV_TMP114NB` in libusdr).
    pub(crate) const fn usdr() -> Self {
        Self::at(I2cAddr::new(0, 0x4e))
    }

    /// Reads the temperature in degrees Celsius.
    pub(crate) fn celsius(self, bus: &mut dyn Bus) -> Result<f32, Error> {
        let (mut operation, request) = TemperatureRead::begin(self);
        let mut raw = [0; 2];
        let result = bus
            .i2c(request.dev, &request.register, &mut raw)
            .map(|()| raw.as_slice());
        operation
            .complete(result)
            .expect("invariant: a fresh temperature operation completes exactly once")
    }

    /// Checks the device ID register.
    pub(crate) fn check_id(self, bus: &mut dyn Bus) -> Result<(), Error> {
        let raw = self.read(bus, Reg::DeviceId)?;
        match DeviceId::from_raw(u16::from_be_bytes(raw)) {
            Self::DEVICE_ID => Ok(()),
            found => Err(Error::ChipId {
                chip: Self::NAME,
                expected: Self::DEVICE_ID.to_raw().into(),
                found: found.to_raw().into(),
            }),
        }
    }

    /// Whether the device ID register reads the ID libusdr requires (`tmp114_devid_get`
    /// compared with 0x1114); a missing sensor reads `false`.
    pub(crate) fn identifies(self, bus: &mut dyn Bus) -> Result<bool, Error> {
        let raw = self.read(bus, Reg::DeviceId)?;
        Ok(DeviceId::from_raw(u16::from_be_bytes(raw)) == Self::DEVICE_ID)
    }

    /// Reads one 16-bit register, in wire order.
    fn read(self, bus: &mut dyn Bus, reg: Reg) -> Result<[u8; 2], Error> {
        let reg = reg.into();
        let mut raw = [0; 2];
        bus.i2c(self.dev, &[reg], &mut raw)
            .reading(Self::NAME, reg)?;
        Ok(raw)
    }
}

/// Host error/context adapter around the shared, platform-independent temperature machine.
pub(crate) struct TemperatureRead(Operation);

impl TemperatureRead {
    /// Begins a read and returns the action for a blocking, readiness or callback executor.
    pub(crate) fn begin(sensor: Tmp114) -> (Self, TemperatureRequest) {
        let (operation, request) = Operation::begin(sensor.dev);
        (Self(operation), request)
    }

    /// Resolves a read that has not completed yet; see [`Self::complete`].
    #[cfg(all(feature = "mio", target_os = "linux"))]
    pub(crate) fn resolve(&mut self, result: Result<&[u8], BusError>) -> Result<f32, Error> {
        self.complete(result)
            .expect("invariant: each native read resolves once")
    }

    /// Fails a read that has not completed yet with a transport error, with its context.
    #[cfg(all(feature = "mio", target_os = "linux"))]
    pub(crate) fn fail(&mut self, error: BusError) -> Error {
        self.resolve(Err(error))
            .expect_err("invariant: a failed transport yields no temperature")
    }

    /// Resolves the read once, preserving transport errors and rejecting malformed data.
    pub(crate) fn complete(
        &mut self,
        result: Result<&[u8], BusError>,
    ) -> Option<Result<f32, Error>> {
        self.0.complete(result).map(|result| {
            result
                .map_err(BusError::from)
                .reading(Tmp114::NAME, u8::from(Reg::Temperature))
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;
    use crate::error::Access;

    #[test]
    fn temperature_action_and_signed_wire_values_follow_tmp114_datasheet() {
        // SNIS214E §8.6.1: signed big-endian counts, 1/128 °C per count.
        for (wire, expected) in [
            ([0x00, 0x00], 0.0),
            ([0x0c, 0x80], 25.0),
            ([0xe4, 0x80], -55.0),
            ([0xff, 0xff], -1.0 / 128.0),
            ([0x00, 0x01], 1.0 / 128.0),
            ([0x7f, 0xff], 255.992_19),
            ([0x80, 0x00], -256.0),
        ] {
            let (mut operation, request) = TemperatureRead::begin(Tmp114::usdr());
            assert_eq!((request.dev.bus(), request.dev.addr()), (0, 0x4e));
            assert_eq!(request.register, [0]);
            let actual = operation.complete(Ok(&wire)).unwrap().unwrap();
            assert!((actual - expected).abs() < f32::EPSILON, "wire {wire:?}");
            assert!(operation.complete(Ok(&wire)).is_none());
        }
    }

    #[test]
    fn malformed_temperature_reply_is_terminal_and_preserves_read_context() {
        for bytes in [&[][..], &[0][..], &[0, 0, 0][..], &[0; 4][..]] {
            let (mut operation, _) = TemperatureRead::begin(Tmp114::usdr());
            assert!(matches!(
                operation.complete(Ok(bytes)),
                Some(Err(Error::Bus {
                    chip: "TMP114",
                    reg: 0,
                    access: Access::Read,
                    source: BusError::Io(ref error),
                    ..
                })) if error.kind() == io::ErrorKind::InvalidData
            ));
            assert!(operation.complete(Ok(&[0x0c, 0x80])).is_none());
        }
    }

    #[test]
    fn transport_failure_cannot_be_replaced_by_a_late_success() {
        let (mut operation, _) = TemperatureRead::begin(Tmp114::usdr());
        assert!(matches!(
            operation.complete(Err(BusError::Timeout)),
            Some(Err(Error::Bus {
                chip: "TMP114",
                reg: 0,
                source: BusError::Timeout,
                ..
            }))
        ));
        assert!(operation.complete(Ok(&[0x0c, 0x80])).is_none());
    }

    #[test]
    fn native_io_source_and_chip_context_survive_the_portable_boundary() {
        let (mut operation, _) = TemperatureRead::begin(Tmp114::usdr());
        assert!(matches!(
            operation.complete(Err(BusError::Io(io::Error::from_raw_os_error(5)))),
            Some(Err(Error::Bus {
                chip: "TMP114",
                reg: 0,
                access: Access::Read,
                source: BusError::Io(ref error),
            })) if error.raw_os_error() == Some(5)
        ));
    }
}
