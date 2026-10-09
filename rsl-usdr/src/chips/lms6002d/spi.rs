//! The LMS6002D SPI protocol: one 16-bit word per register access.
//!
//! The FPGA's SPI engine sends the word and returns the chip's reply; for a read, the
//! register's byte comes back in the low eight bits. The 7-bit address reaches 128 byte
//! registers, grouped into blocks by address range.

use bnb::{bitfield, u7};

use crate::chips::register::{IndexedRegister, Register};
use crate::error::{BusContext, Error};
use crate::lowlevel::{Bus, SpiAddr};

/// An LMS6002D block's register-address enum; only these address an [`SpiWord`].
pub(super) trait BlockReg: Into<u8> {}

/// One SPI transaction word.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SpiWord {
    /// Set for a write, clear for a read.
    #[bits(15..=15)]
    write: bool,

    /// Register address.
    #[bits(8..=14)]
    addr: u7,

    /// The byte written; zero for reads.
    #[bits(0..=7)]
    data: u8,
}

/// The LMS6002D's registers behind one SPI target, one [`SpiWord`] per access.
#[derive(Clone, Copy, Debug)]
pub(super) struct SpiRegisters {
    /// SPI target the chip is on.
    target: SpiAddr,
}

impl SpiRegisters {
    /// Registers of the chip on `target`.
    pub(super) const fn new(target: SpiAddr) -> Self {
        Self { target }
    }

    /// These registers on `bus`, for the accesses of one operation.
    pub(super) fn on(self, bus: &mut dyn Bus) -> SpiWriter<'_> {
        SpiWriter { regs: self, bus }
    }
}

/// [`SpiRegisters`] bound to a bus for one operation; it reads and writes, one [`SpiWord`]
/// per access.
pub(super) struct SpiWriter<'a> {
    /// Which chip.
    regs: SpiRegisters,

    /// The bus the chip is on.
    bus: &'a mut dyn Bus,
}

impl SpiWriter<'_> {
    /// Reads a typed register; its byte comes back in the reply's low eight bits.
    pub(super) fn read<R: Register<Map: BlockReg>>(&mut self) -> Result<R, Error> {
        let word = SpiWord::new().with_addr(u7::new(R::ADDR.into()));

        let reply = self
            .bus
            .spi32(self.regs.target, word.to_raw().into())
            .during("LMS6002D read")?;

        Ok(R::from(reply.to_le_bytes()[0]))
    }

    /// Writes a typed register.
    pub(super) fn write<R: Register<Map: BlockReg>>(&mut self, value: R) -> Result<(), Error> {
        self.write_raw(R::ADDR, value.into())
    }

    /// Writes one copy of a repeated register.
    pub(super) fn write_to<R: IndexedRegister<Map: BlockReg>>(
        &mut self,
        index: R::Index,
        value: R,
    ) -> Result<(), Error> {
        self.write_raw(R::addr(index), value.into())
    }

    /// Writes a register as a byte.
    pub(super) fn write_raw(&mut self, reg: impl BlockReg, value: u8) -> Result<(), Error> {
        let word = SpiWord::new()
            .with_write(true)
            .with_addr(u7::new(reg.into()))
            .with_data(value);

        self.bus
            .spi32(self.regs.target, word.to_raw().into())
            .during("LMS6002D write")?;

        Ok(())
    }
}
