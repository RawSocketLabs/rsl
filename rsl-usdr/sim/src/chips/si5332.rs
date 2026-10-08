//! Si5332 clock generator: an 8-bit register file whose status follows the system control.

use super::{I2cChip, Reg8File};

/// System control register: 1 = READY (outputs held), 2 = ACTIVE.
const USYS_CTRL: u8 = 0x06;
/// System status register; mirrors the requested state while an input clock is present.
const USYS_STAT: u8 = 0x07;
/// System status without an input clock.
const NO_INPUT_CLOCK: u8 = 0x89;

/// A Si5332 that reaches every requested state immediately when it has an input clock.
#[derive(Debug)]
pub(crate) struct Si5332 {
    /// Underlying register contents.
    pub(crate) file: Reg8File,
    /// Whether the reference input is running; set by the board before each transfer.
    pub(crate) input_clock: bool,
}

impl Si5332 {
    /// A Si5332 in READY state with its input clock running.
    pub(crate) fn new() -> Self {
        Self {
            file: Reg8File::with_resets(&[(USYS_CTRL, 0x01), (USYS_STAT, 0x01)]),
            input_clock: true,
        }
    }

    /// Recomputes the status register from the control register and the input clock.
    fn sync_status(&mut self) {
        let ctrl = self.file.regs[usize::from(USYS_CTRL)];
        self.file.regs[usize::from(USYS_STAT)] = if self.input_clock {
            ctrl
        } else {
            NO_INPUT_CLOCK
        };
    }
}

impl I2cChip for Si5332 {
    fn transfer(&mut self, write: &[u8], read_len: usize) -> Vec<u8> {
        self.sync_status();
        let read = self.file.transfer(write, read_len);
        self.sync_status();
        read
    }
}
