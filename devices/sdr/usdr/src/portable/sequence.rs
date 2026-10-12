//! What a portable register sequence asks of its executor, and how it fails.
//!
//! A sequence is a hand-written state machine: [`Sequence::next`] names one action, the
//! executor performs it and reports the outcome to [`Sequence::complete`]. The blocking
//! driver and any other executor run the same machines, so each sequence exists once.

use core::time::Duration;

use super::address::I2cAddr;

/// One single-byte register access on an I²C chip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RegisterAccess {
    /// Write the register address, then read one byte.
    Read {
        /// Where the chip answers.
        dev: I2cAddr,
        /// The register.
        reg: u8,
    },
    /// Write the register address and one byte.
    Write {
        /// Where the chip answers.
        dev: I2cAddr,
        /// The register.
        reg: u8,
        /// The byte written.
        value: u8,
    },
}

impl RegisterAccess {
    /// Reads `reg` on `dev`.
    pub(super) fn read(dev: I2cAddr, reg: impl Into<u8>) -> Self {
        Self::Read {
            dev,
            reg: reg.into(),
        }
    }

    /// Writes an `(address, byte)` entry on `dev`.
    pub(super) fn write<M: Into<u8>>(dev: I2cAddr, (reg, value): (M, u8)) -> Self {
        Self::Write {
            dev,
            reg: reg.into(),
            value,
        }
    }
}

/// Why a register sequence stopped. `E` is the executor's transport error, kept unchanged.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SequenceFailure<E> {
    /// The access failed in the executor's transport.
    Transport {
        /// The chip's name.
        chip: &'static str,
        /// The access that failed.
        access: RegisterAccess,
        /// The executor's error.
        source: E,
    },
    /// A chip identified itself with an unexpected ID.
    ChipId {
        /// The chip's name.
        chip: &'static str,
        /// The ID the sequence requires.
        expected: u32,
        /// The ID read.
        found: u32,
    },
    /// No chip answered where one must be.
    ChipMissing {
        /// The chip's name.
        chip: &'static str,
    },
    /// A clock generator reported no input clock, so it cannot run.
    ClockInputMissing,
}

/// One thing a sequence needs done next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SequenceAction {
    /// A register access: report the byte read, or any byte for a write.
    Access(RegisterAccess),
    /// Wait at least this long, then report any byte.
    Sleep(Duration),
}

/// A register sequence an executor drives one action at a time.
pub(crate) trait Sequence {
    /// The action to perform next; `None` once the sequence finished or failed.
    fn next(&self) -> Option<SequenceAction>;

    /// Applies the outcome of [`Self::next`]'s action. A failure ends the sequence; an
    /// outcome with no action pending is ignored, like a duplicate.
    fn complete<E>(&mut self, result: Result<u8, E>) -> Result<(), SequenceFailure<E>>;
}
