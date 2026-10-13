//! Register access for the blocking drivers: `rsl-lowlevel`'s executor with the driver's
//! errors, and blocking I2C writes. The typed-register traits ([`Register`],
//! [`IndexedRegister`]) are `rsl-lowlevel`'s, shared with the sans-I/O machines.

use rsl_lowlevel::run_sequence as run_on_bus;

use crate::error::{BusContext, Error};
use crate::lowlevel::{Bus, I2cAddr};
pub(crate) use crate::portable::register::{IndexedRegister, Register};
use crate::portable::sequence::Sequence;

/// Runs a portable register sequence to its end over `bus`: `rsl-lowlevel`'s blocking
/// executor, with the failure in the driver's terms.
pub(crate) fn run_sequence(bus: &mut dyn Bus, sequence: &mut impl Sequence) -> Result<(), Error> {
    run_on_bus(bus, sequence).map_err(Error::from)
}

/// A chip whose byte-wide registers sit behind an I2C device with an auto-incrementing
/// register pointer. Visible only to chip modules, so board code reaches the registers
/// through the driver's methods.
pub(in crate::chips) trait I2cChip {
    /// The chip's name in errors.
    const NAME: &'static str;

    /// Where the chip answers.
    fn addr(&self) -> I2cAddr;

    /// This chip's registers on `bus`, for the accesses of one operation.
    fn on<'a>(&self, bus: &'a mut dyn Bus) -> I2cWriter<'a> {
        I2cWriter {
            dev: self.addr(),
            chip: Self::NAME,
            bus,
        }
    }
}

/// An [`I2cChip`]'s registers bound to a bus for one operation; it writes. Reads run in
/// portable sequences.
pub(in crate::chips) struct I2cWriter<'a> {
    /// Where the chip answers.
    dev: I2cAddr,

    /// Names the chip in errors.
    chip: &'static str,

    /// The bus the chip is on.
    bus: &'a mut dyn Bus,
}

impl I2cWriter<'_> {
    /// Writes a typed register.
    pub(in crate::chips) fn write<R: Register>(&mut self, value: R) -> Result<(), Error> {
        self.write_raw(R::ADDR, value.into())
    }

    /// Writes one copy of a repeated register.
    pub(in crate::chips) fn write_to<R: IndexedRegister>(
        &mut self,
        index: R::Index,
        value: R,
    ) -> Result<(), Error> {
        self.write_raw(R::addr(index), value.into())
    }

    /// Writes a register as a byte.
    pub(in crate::chips) fn write_raw(
        &mut self,
        reg: impl Into<u8>,
        value: u8,
    ) -> Result<(), Error> {
        let reg = reg.into();
        self.bus
            .i2c(self.dev, &[reg, value], &mut [])
            .writing(self.chip, reg)
    }
}
