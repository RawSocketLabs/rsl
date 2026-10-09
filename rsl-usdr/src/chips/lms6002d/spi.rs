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

/// The LMS6002D's registers bound to a bus for one operation; it reads and writes, one
/// [`SpiWord`] per access.
pub(super) struct SpiWriter<'a> {
    /// SPI target the chip is on.
    target: SpiAddr,

    /// Names the chip in errors.
    chip: &'static str,

    /// The bus the chip is on.
    bus: &'a mut dyn Bus,
}

impl<'a> SpiWriter<'a> {
    /// The registers of `chip` on `target`, over `bus`.
    pub(super) fn new(target: SpiAddr, chip: &'static str, bus: &'a mut dyn Bus) -> Self {
        Self { target, chip, bus }
    }
}

impl SpiWriter<'_> {
    /// Reads a typed register; its byte comes back in the reply's low eight bits.
    pub(super) fn read<R: Register<Map: BlockReg>>(&mut self) -> Result<R, Error> {
        let reg = R::ADDR.into();
        let word = SpiWord::new().with_addr(u7::new(reg));

        let reply = self
            .bus
            .spi32(self.target, word.to_raw().into())
            .reading(self.chip, reg)?;

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
        let reg = reg.into();
        let word = SpiWord::new()
            .with_write(true)
            .with_addr(u7::new(reg))
            .with_data(value);

        self.bus
            .spi32(self.target, word.to_raw().into())
            .writing(self.chip, reg)?;

        Ok(())
    }
}
