//! Register-level models of the chips on the LMS7 family boards. The LMS7002M model is
//! shared with the other LMS7002M boards' simulators, in `lms7002m-sim`.

mod dac80501;
mod lp8758;

pub(crate) use dac80501::Dac80501;
pub(crate) use lp8758::Lp8758;

/// A chip on an I2C bus, addressed with a register pointer as the first written byte.
pub(crate) trait I2cChip {
    /// Performs a write-then-read transfer: `write` is the bytes sent (register pointer
    /// first), and the returned bytes are what the chip transmits for `read_len` bytes.
    fn transfer(&mut self, write: &[u8], read_len: usize) -> Vec<u8>;
}
