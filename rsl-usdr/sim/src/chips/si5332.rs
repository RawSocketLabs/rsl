//! Si5332 clock generator: an 8-bit register file whose status follows the system control.

use super::{I2cChip, Reg8File};

/// System control register: 1 = READY (outputs held), 2 = ACTIVE.
const USYS_CTRL: u8 = 0x06;
/// System status register; mirrors the requested state once the input clock is valid.
const USYS_STAT: u8 = 0x07;

/// A Si5332 that reaches every requested state immediately.
#[derive(Debug)]
pub(crate) struct Si5332 {
    /// Underlying register contents.
    pub(crate) file: Reg8File,
}

impl Si5332 {
    /// A Si5332 in READY state.
    pub(crate) fn new() -> Self {
        Self {
            file: Reg8File::with_resets(&[(USYS_CTRL, 0x01), (USYS_STAT, 0x01)]),
        }
    }
}

impl I2cChip for Si5332 {
    fn transfer(&mut self, write: &[u8], read_len: usize) -> Vec<u8> {
        let read = self.file.transfer(write, read_len);
        if let [USYS_CTRL, state, ..] = *write {
            self.file.regs[usize::from(USYS_STAT)] = state;
        }
        read
    }
}
