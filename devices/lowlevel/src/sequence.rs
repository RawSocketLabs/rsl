//! What a portable register sequence asks of its executor, and how it fails.
//!
//! A sequence is a hand-written state machine: [`Sequence::next`] names one action, the
//! executor performs it and reports the outcome to [`Sequence::complete`]. The blocking
//! driver and any other executor run the same machines, so each sequence exists once.
//! Actions are I²C chip registers, FPGA (gateware) registers, SPI exchanges and waits.

use core::fmt;
use core::time::Duration;

use super::address::{I2cAddr, SpiAddr};

/// The direction of a failed register access.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    /// A register read.
    Read,

    /// A register write.
    Write,
}

impl fmt::Display for Access {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Read => "read",
            Self::Write => "write",
        })
    }
}

/// One single-byte register access on an I²C chip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegisterAccess {
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
    pub fn read(dev: I2cAddr, reg: impl Into<u8>) -> Self {
        Self::Read {
            dev,
            reg: reg.into(),
        }
    }

    /// Writes an `(address, byte)` entry on `dev`.
    pub fn write<M: Into<u8>>(dev: I2cAddr, (reg, value): (M, u8)) -> Self {
        Self::Write {
            dev,
            reg: reg.into(),
            value,
        }
    }

    /// The direction and register of this access, as a failure report names them.
    #[must_use]
    pub fn context(&self) -> (Access, u32) {
        match *self {
            Self::Read { reg, .. } => (Access::Read, reg.into()),
            Self::Write { reg, .. } => (Access::Write, reg.into()),
        }
    }
}

/// One 32-bit access through the FPGA: a gateware register, numbered as the blocking `Bus`
/// numbers them (libusdr's `ls_op` register address), or a word shifted through an SPI
/// target.
///
/// A register past a transport's physical space, such as the uSDR stream engine's
/// configuration space from 0x10000, is reached the way that transport reaches it (through
/// an index window): the access names the register, not the transport's steps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FpgaAccess {
    /// Read one gateware register.
    Read {
        /// The register.
        reg: u32,
    },
    /// Write one gateware register.
    Write {
        /// The register.
        reg: u32,
        /// The word written.
        value: u32,
    },
    /// Shift `word` through `target` and report the word read back.
    Spi {
        /// The SPI target.
        target: SpiAddr,
        /// The word shifted out.
        word: u32,
        /// The target chip's register the word addresses (up to the LMS7002M's 15 bits),
        /// for error context only.
        reg: u32,
        /// Whether the word writes that register, for error context only.
        write: bool,
    },
}

impl FpgaAccess {
    /// The direction and register of this access, as a failure report names them: the
    /// gateware register, or the SPI target chip's own register.
    #[must_use]
    pub fn context(&self) -> (Access, u32) {
        match *self {
            Self::Read { reg } => (Access::Read, reg),
            Self::Write { reg, .. } => (Access::Write, reg),
            Self::Spi { reg, write, .. } => (if write { Access::Write } else { Access::Read }, reg),
        }
    }
}

/// Why a register sequence stopped. `E` is the executor's transport error, kept unchanged.
#[derive(Debug, PartialEq, Eq)]
pub enum SequenceFailure<E> {
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
    /// A chip reported no input signal it needs to run, such as a clock generator without
    /// its input clock.
    InputMissing {
        /// The chip's name.
        chip: &'static str,
    },
}

impl<E> SequenceFailure<E> {
    /// The chip, direction and register a failed access names; `None` for failures that
    /// are not an access (wrong ID, missing chip or input).
    #[must_use]
    pub fn access(&self) -> Option<(&'static str, Access, u32)> {
        let (chip, (access, reg)) = match self {
            Self::Transport { chip, access, .. } => (*chip, access.context()),
            Self::FpgaTransport { chip, access, .. } => (*chip, access.context()),
            Self::ChipId { .. } | Self::ChipMissing { .. } | Self::InputMissing { .. } => {
                return None;
            }
        };
        Some((chip, access, reg))
    }
}

/// One thing a sequence needs done next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SequenceAction {
    /// An I²C register access: report the byte read, or any value for a write.
    Access(RegisterAccess),
    /// An FPGA register access or SPI exchange: report the word read, or any value for a
    /// register write.
    Fpga(FpgaAccess),
    /// Wait at least this long, then report any value.
    Sleep(Duration),
}

/// A register sequence an executor drives one action at a time.
pub trait Sequence {
    /// The action to perform next; `None` once the sequence finished or failed.
    fn next(&self) -> Option<SequenceAction>;

    /// Applies the outcome of [`Self::next`]'s action. A failure ends the sequence; an
    /// outcome with no action pending is ignored, like a duplicate. An I²C byte arrives
    /// zero-extended (see [`i2c_byte`]).
    ///
    /// # Errors
    ///
    /// The sequence's failure: the executor's error with the access it struck, or a fault
    /// the sequence found in what it read.
    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>>;
}

/// The byte an I²C register action reported: executors report it zero-extended, so byte-wide
/// machines keep the low eight bits.
///
/// # Errors
///
/// The executor's error, unchanged.
pub fn i2c_byte<E>(result: Result<u32, E>) -> Result<u8, E> {
    result.map(|word| word.to_le_bytes()[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_i2c_access_names_its_chip_register_and_direction() {
        let dev = I2cAddr::new(0, 0x75);
        let read = SequenceFailure::Transport {
            chip: "TPS6381x",
            access: RegisterAccess::read(dev, 0x03),
            source: (),
        };
        assert_eq!(read.access(), Some(("TPS6381x", Access::Read, 0x03)));
        let write = SequenceFailure::Transport {
            chip: "TPS6381x",
            access: RegisterAccess::write(dev, (0x01, 0x28)),
            source: (),
        };
        assert_eq!(write.access(), Some(("TPS6381x", Access::Write, 0x01)));
    }

    #[test]
    fn a_failed_spi_exchange_names_the_target_chips_wide_register() {
        let failure = SequenceFailure::FpgaTransport {
            chip: "LMS7002M",
            access: FpgaAccess::Spi {
                target: SpiAddr(0),
                word: 0x0000_0000,
                reg: 0x7fff,
                write: false,
            },
            source: (),
        };
        assert_eq!(failure.access(), Some(("LMS7002M", Access::Read, 0x7fff)));
        let write = FpgaAccess::Write { reg: 19, value: 1 };
        assert_eq!(write.context(), (Access::Write, 19));
    }

    #[test]
    fn faults_that_are_not_accesses_name_no_register() {
        let missing = SequenceFailure::<()>::InputMissing { chip: "Si5332" };
        assert_eq!(missing.access(), None);
        let id = SequenceFailure::<()>::ChipId {
            chip: "LP8758",
            expected: 0xe001,
            found: 0,
        };
        assert_eq!(id.access(), None);
    }
}
