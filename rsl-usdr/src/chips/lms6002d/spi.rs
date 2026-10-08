//! The LMS6002D SPI protocol: one 16-bit word per register access.

use bnb::{bitfield, u7};

use crate::chips::register::{IndexedRegister, Register};

/// An LMS6002D block's register-address enum; only these address an [`SpiWord`].
pub(super) trait BlockReg: Into<u8> {}

/// One SPI transaction word.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SpiWord {
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

impl SpiWord {
    /// A write of a raw byte to `reg`.
    pub(super) fn store_raw(reg: impl BlockReg, value: u8) -> Self {
        Self::new()
            .with_write(true)
            .with_addr(u7::new(reg.into()))
            .with_data(value)
    }

    /// A write of a typed register.
    pub(super) fn store<R: Register<Map: BlockReg>>(value: R) -> Self {
        Self::store_raw(R::ADDR, value.to_byte())
    }

    /// A write of one copy of a repeated register.
    pub(super) fn store_at<R: IndexedRegister<Map: BlockReg>>(index: R::Index, value: R) -> Self {
        Self::store_raw(R::addr(index), value.to_byte())
    }

    /// A read of a typed register's address.
    pub(super) fn load<R: Register<Map: BlockReg>>() -> Self {
        Self::new().with_addr(u7::new(R::ADDR.into()))
    }
}
