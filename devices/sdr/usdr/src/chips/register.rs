//! Register access for the blocking drivers. The typed-register traits live in the
//! portable core ([`Register`], [`IndexedRegister`]), shared with its sans-I/O machines.

use crate::error::{BusContext, Error};
use crate::lowlevel::{Bus, I2cAddr};
pub(crate) use crate::portable::register::{IndexedRegister, Register};
use crate::portable::sequence::{RegisterAccess, Sequence, SequenceAction};

/// Runs a portable register sequence to its end over `bus`, one action at a time: the
/// blocking executor, so the sequence's operations reach the bus exactly as written.
pub(crate) fn run_sequence(bus: &mut dyn Bus, sequence: &mut impl Sequence) -> Result<(), Error> {
    while let Some(action) = sequence.next() {
        let result = match action {
            SequenceAction::Access(RegisterAccess::Read { dev, reg }) => {
                let mut byte = [0];
                bus.i2c(dev, &[reg], &mut byte).map(|()| byte[0])
            }
            SequenceAction::Access(RegisterAccess::Write { dev, reg, value }) => {
                bus.i2c(dev, &[reg, value], &mut []).map(|()| value)
            }
            SequenceAction::Sleep(duration) => {
                bus.sleep(duration);
                Ok(0)
            }
        };
        sequence.complete(result)?;
    }
    Ok(())
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
