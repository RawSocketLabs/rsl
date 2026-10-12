//! What a portable register sequence asks of its executor, and how it fails.
//!
//! A sequence is a hand-written state machine: [`Sequence::next`] names one action, the
//! executor performs it and reports the outcome to [`Sequence::complete`]. The blocking
//! driver and any other executor run the same machines, so each sequence exists once.
//! Actions are I²C chip registers, FPGA (gateware) registers, SPI exchanges and waits.

use core::time::Duration;

use super::address::{I2cAddr, SpiAddr};

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

/// One 32-bit access through the FPGA: a physical gateware register below the stream
/// engine's index window (`0..=0xFFFF`), or a word shifted through an SPI target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "only the native executor's identity read (feature `mio`) issues one yet"
    )
)]
pub(crate) enum FpgaAccess {
    /// Read one gateware register.
    Read {
        /// The physical register.
        reg: u16,
    },
    /// Write one gateware register.
    Write {
        /// The physical register.
        reg: u16,
        /// The word written.
        value: u32,
    },
    /// Shift `word` through `target` and report the word read back.
    Spi {
        /// The SPI target.
        target: SpiAddr,
        /// The word shifted out.
        word: u32,
        /// The target chip's register the word addresses, for error context only.
        reg: u8,
        /// Whether the word writes that register, for error context only.
        write: bool,
    },
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
    /// An FPGA register access or SPI exchange failed in the executor's transport.
    FpgaTransport {
        /// The chip's name: `"FPGA"` for gateware registers, the SPI target otherwise.
        chip: &'static str,
        /// The access that failed.
        access: FpgaAccess,
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
    /// An I²C register access: report the byte read, or any value for a write.
    Access(RegisterAccess),
    /// An FPGA register access or SPI exchange: report the word read, or any value for a
    /// register write.
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "only the native executor's sequences (feature `mio`) touch the FPGA yet"
        )
    )]
    Fpga(FpgaAccess),
    /// Wait at least this long, then report any value.
    Sleep(Duration),
}

/// A register sequence an executor drives one action at a time.
pub(crate) trait Sequence {
    /// The action to perform next; `None` once the sequence finished or failed.
    fn next(&self) -> Option<SequenceAction>;

    /// Applies the outcome of [`Self::next`]'s action. A failure ends the sequence; an
    /// outcome with no action pending is ignored, like a duplicate. An I²C byte arrives
    /// zero-extended (see [`i2c_byte`]).
    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>>;
}

/// The byte an I²C register action reported: executors report it zero-extended, so byte-wide
/// machines keep the low eight bits.
pub(crate) fn i2c_byte<E>(result: Result<u32, E>) -> Result<u8, E> {
    result.map(|word| word.to_le_bytes()[0])
}
