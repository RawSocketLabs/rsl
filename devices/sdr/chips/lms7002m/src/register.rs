//! Typed 16-bit registers: a value type names its address.
//!
//! `rsl-lowlevel`'s [`Register`](rsl_lowlevel::Register) traits are 8-bit, so the LMS7002M,
//! with 15-bit addresses and 16-bit registers, has its own, as `usdr`'s LMK05318 does.

use super::reg::Reg;

/// A 16-bit LMS7002M register at one fixed address. Registers the chip banks per channel
/// (the `MAC` selection in [`LmlControl`](crate::LmlControl)) use the same type for every
/// bank; the write goes to the banks `MAC` selects.
pub trait Register: Copy + Into<u16> {
    /// Where this register lives.
    const ADDR: Reg;

    /// This value as an `(address, value)` write.
    fn entry(self) -> (Reg, u16) {
        (Self::ADDR, self.into())
    }
}
