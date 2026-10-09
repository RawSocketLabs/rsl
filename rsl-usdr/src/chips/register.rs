//! Typed registers: a register's value type names its address.
//!
//! A chip module declares a `Reg` enum for every address it touches. Registers whose
//! contents the driver interprets get a value type (a `bnb` bitfield or `BitEnum`) that
//! implements [`Register`] or, when the chip repeats it per channel or output,
//! [`IndexedRegister`]. Writing a value then needs no address argument, and the wrong value
//! for a register does not compile. Registers written as opaque bytes stay `(Reg, u8)`.

use crate::error::{BusContext, Error};
use crate::lowlevel::{Bus, I2cAddr};

/// An 8-bit register at one fixed address.
///
/// The byte conversions are `From`: `bnb` generates them for a bitfield and for a
/// whole-byte `BitEnum` with a catch-all.
pub(crate) trait Register: Copy + From<u8> + Into<u8> {
    /// The chip's register-address enum.
    type Map: Into<u8>;
    /// Where this register lives.
    const ADDR: Self::Map;

    /// This value as an `(address, byte)` write.
    fn entry(self) -> (Self::Map, u8) {
        (Self::ADDR, self.into())
    }
}

/// An 8-bit register the chip repeats; `Index` (a channel or output) selects the copy.
pub(crate) trait IndexedRegister: Copy + Into<u8> {
    /// The chip's register-address enum.
    type Map: Into<u8>;
    /// What selects one copy of the register.
    type Index: Copy;
    /// Where the copy for `index` lives.
    fn addr(index: Self::Index) -> Self::Map;

    /// This value, for the copy at `index`, as an `(address, byte)` write.
    fn entry_at(self, index: Self::Index) -> (Self::Map, u8) {
        (Self::addr(index), self.into())
    }
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

/// An [`I2cChip`]'s registers bound to a bus for one operation; it reads and writes.
pub(in crate::chips) struct I2cWriter<'a> {
    /// Where the chip answers.
    dev: I2cAddr,

    /// Names the chip in errors.
    chip: &'static str,

    /// The bus the chip is on.
    bus: &'a mut dyn Bus,
}

impl I2cWriter<'_> {
    /// Reads a typed register.
    pub(in crate::chips) fn read<R: Register>(&mut self) -> Result<R, Error> {
        self.read_raw(R::ADDR).map(R::from)
    }

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

    /// Reads a register as a byte.
    pub(in crate::chips) fn read_raw(&mut self, reg: impl Into<u8>) -> Result<u8, Error> {
        let reg = reg.into();
        let mut value = [0];
        self.bus
            .i2c(self.dev, &[reg], &mut value)
            .reading(self.chip, reg)?;
        Ok(value[0])
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
