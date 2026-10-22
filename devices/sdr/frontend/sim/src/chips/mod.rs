//! Register-level models of the front-end boards' chips, and the I2C seam they share with
//! the host boards' own chips.

mod dac80501;
mod expander;
mod reg16;
mod reg8;
mod tmp114;

pub(crate) use dac80501::Dac80501;
pub(crate) use expander::Expander;
/// The `SimpleSync` and XMASS front ends' LMK05318, the model `PE_SYNC`'s sim shares.
pub(crate) use lmk05318_sim::Lmk05318;
pub use reg8::Reg8File;
pub(crate) use reg16::Reg16File;
pub use tmp114::Tmp114;

/// A chip on an I2C bus, addressed with a register pointer as the first written byte.
pub trait I2cChip {
    /// Performs a write-then-read transfer: `write` is the bytes sent (register pointer
    /// first), and the returned bytes are what the chip transmits for `read_len` bytes.
    fn transfer(&mut self, write: &[u8], read_len: usize) -> Vec<u8>;
}

impl I2cChip for Lmk05318 {
    fn transfer(&mut self, write: &[u8], read_len: usize) -> Vec<u8> {
        Self::transfer(self, write, read_len)
    }
}
