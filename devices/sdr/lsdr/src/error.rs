//! The driver's error type.

pub use rsl_lowlevel::Access;
use rsl_lowlevel::SequenceFailure;

use crate::lowlevel::BusError;

/// A failed driver operation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A register access failed on the bus.
    #[error("{chip} register {reg:#06x} {access} failed")]
    #[non_exhaustive]
    Bus {
        /// The chip accessed: `"FPGA"` (gateware registers), `"TPS6594"`, `"Si549"`,
        /// `"ADS42LBx9"` or `"LMK04832"`.
        chip: &'static str,

        /// Whether it was a read or a write.
        access: Access,

        /// The register address, in the chip's own address space.
        reg: u32,

        /// The bus failure.
        #[source]
        source: BusError,
    },

    /// The board's status register read all ones, libusdr's "Device is dead!": the link
    /// reaches no live gateware. Nothing was driven.
    #[error("the LSDr does not answer: its status register reads all ones")]
    DeadLink,

    /// A chip identified itself with an unexpected ID. Not produced yet: no ported sequence
    /// checks an ID it requires; it keeps `rsl-lowlevel`'s sequence failures mapped one to
    /// one.
    #[error("{chip} reports ID {found:#x}, expected {expected:#x}")]
    #[non_exhaustive]
    ChipId {
        /// Which chip.
        chip: &'static str,

        /// The ID the driver requires.
        expected: u32,

        /// The ID read.
        found: u32,
    },

    /// No chip answered where one must be. Not produced yet, as [`Self::ChipId`]: an
    /// absent ADC or clock generator is reported by [`Device::fitted`](crate::Device::fitted).
    #[error("{0} not found")]
    ChipMissing(&'static str),

    /// A chip reported no input signal it needs. Not produced yet, as [`Self::ChipId`].
    #[error("{0} has no input signal")]
    InputMissing(&'static str),
}

/// A failed portable register sequence, with the register or chip it names.
impl From<SequenceFailure<BusError>> for Error {
    fn from(failure: SequenceFailure<BusError>) -> Self {
        let bus = |chip, (access, reg), source| Self::Bus {
            chip,
            access,
            reg,
            source,
        };
        match failure {
            SequenceFailure::Transport {
                chip,
                access,
                source,
            } => bus(chip, access.context(), source),
            SequenceFailure::FpgaTransport {
                chip,
                access,
                source,
            } => bus(chip, access.context(), source),
            SequenceFailure::ChipId {
                chip,
                expected,
                found,
            } => Self::ChipId {
                chip,
                expected,
                found,
            },
            SequenceFailure::ChipMissing { chip } => Self::ChipMissing(chip),
            SequenceFailure::InputMissing { chip } => Self::InputMissing(chip),
        }
    }
}

#[cfg(test)]
mod tests {
    use rsl_lowlevel::{FpgaAccess, SpiAddr};

    use super::*;

    #[test]
    fn a_failed_spi_exchange_names_the_adc_register_not_the_word() {
        let failure = SequenceFailure::FpgaTransport {
            chip: "ADS42LBx9",
            access: FpgaAccess::Spi {
                target: SpiAddr(1),
                word: 0x8d00,
                reg: 0x0d,
                write: false,
            },
            source: BusError::Disconnected,
        };
        assert!(matches!(
            Error::from(failure),
            Error::Bus {
                chip: "ADS42LBx9",
                access: Access::Read,
                reg: 0x0d,
                source: BusError::Disconnected,
            }
        ));
    }
}
