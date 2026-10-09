//! Register-level models of the chips on the uSDR board.

mod lms6002d;
mod reg8;
mod si5332;
mod tmp114;

pub(crate) use lms6002d::Lms6002d;
pub use lms6002d::{DcCalibration, PllLock};
pub(crate) use reg8::Reg8File;
pub(crate) use si5332::Si5332;
pub(crate) use tmp114::Tmp114;

/// A chip on an I2C bus, addressed with a register pointer as the first written byte.
pub(crate) trait I2cChip {
    /// Performs a write-then-read transfer: `write` is the bytes sent (register pointer
    /// first), and the returned bytes are what the chip transmits for `read_len` bytes.
    fn transfer(&mut self, write: &[u8], read_len: usize) -> Vec<u8>;
}
