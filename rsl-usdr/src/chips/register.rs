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

/// Byte-wide registers behind an I2C device with an auto-incrementing register pointer.
#[derive(Clone, Copy, Debug)]
pub(crate) struct I2cRegisters {
    /// Where the chip answers.
    dev: I2cAddr,

    /// Names a failed read in errors.
    read_op: &'static str,

    /// Names a failed write in errors.
    write_op: &'static str,
}

impl I2cRegisters {
    /// Registers of the chip at `dev`; `read_op` and `write_op` name failures.
    pub(crate) const fn new(dev: I2cAddr, read_op: &'static str, write_op: &'static str) -> Self {
        Self {
            dev,
            read_op,
            write_op,
        }
    }

    /// Reads a typed register.
    pub(crate) fn read<R: Register>(self, bus: &mut dyn Bus) -> Result<R, Error> {
        self.read_raw(bus, R::ADDR).map(R::from)
    }

    /// Writes a typed register.
    pub(crate) fn write<R: Register>(self, bus: &mut dyn Bus, value: R) -> Result<(), Error> {
        self.write_raw(bus, R::ADDR, value.into())
    }

    /// Writes one copy of a repeated register.
    pub(crate) fn write_at<R: IndexedRegister>(
        self,
        bus: &mut dyn Bus,
        index: R::Index,
        value: R,
    ) -> Result<(), Error> {
        self.write_raw(bus, R::addr(index), value.into())
    }

    /// Reads a register as a byte.
    pub(crate) fn read_raw(self, bus: &mut dyn Bus, reg: impl Into<u8>) -> Result<u8, Error> {
        let mut value = [0];
        bus.i2c(self.dev, &[reg.into()], &mut value)
            .during(self.read_op)?;
        Ok(value[0])
    }

    /// Writes a register as a byte.
    pub(crate) fn write_raw(
        self,
        bus: &mut dyn Bus,
        reg: impl Into<u8>,
        value: u8,
    ) -> Result<(), Error> {
        bus.i2c(self.dev, &[reg.into(), value], &mut [])
            .during(self.write_op)
    }
}
