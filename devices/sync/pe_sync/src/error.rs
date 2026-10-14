//! The driver's error type.

pub use hal::Access;
use hal::SequenceFailure;
pub use lmk05318::plan::ClockPlanError;

use crate::lowlevel::BusError;

/// A failed driver operation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A register access failed on the bus.
    #[error("{chip} register {reg:#06x} {access} failed")]
    #[non_exhaustive]
    Bus {
        /// The chip accessed: `"FPGA"` (gateware registers), `"LMK05318"`, `"LMX2820"`,
        /// `"LMX1214"`, `"LMX1204"`, `"LMK1D1208I"` or `"LP87524"`.
        chip: &'static str,

        /// Whether it was a read or a write.
        access: Access,

        /// The register address, in the chip's own address space.
        reg: u32,

        /// The bus failure.
        #[source]
        source: BusError,
    },

    /// A chip identified itself with an unexpected ID: the LMK05318's four identity bytes
    /// (as one big-endian word), or the LMK1D1208I's identity byte (`REV_ID << 4 | DEV_ID`).
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

    /// No chip answered where one must be. Not produced yet: no ported sequence reports
    /// it; it keeps `hal`'s sequence failures mapped one to one.
    #[error("{0} not found")]
    ChipMissing(&'static str),

    /// A chip reported no input signal it needs. Not produced yet, as [`Self::ChipMissing`].
    #[error("{0} has no input signal")]
    InputMissing(&'static str),

    /// The LMK05318 has no configuration for the board's clock tree.
    #[error(transparent)]
    ClockPlan(#[from] ClockPlanError),

    /// An LMX2820, the LMX1214 or the LMX1204 has no configuration for the board's clock
    /// tree.
    #[error("{chip} has no configuration: {reason}")]
    #[non_exhaustive]
    NoClockPlan {
        /// Which chip.
        chip: &'static str,

        /// Why.
        reason: &'static str,
    },

    /// A PLL did not report lock within libusdr's timeout: an LMK05318 APLL, an LMX2820, or
    /// the LMX1204's multiplier.
    #[error("{0} did not lock")]
    Unlocked(&'static str),
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
    use hal::{FpgaAccess, SpiAddr};

    use super::*;

    #[test]
    fn a_failed_spi_exchange_names_the_chips_register_not_the_word() {
        let failure = SequenceFailure::FpgaTransport {
            chip: "LMX2820",
            access: FpgaAccess::Spi {
                target: SpiAddr(2),
                word: 0x00ca_0000,
                reg: 0x4a,
                write: false,
            },
            source: BusError::Disconnected,
        };
        assert!(matches!(
            Error::from(failure),
            Error::Bus {
                chip: "LMX2820",
                access: Access::Read,
                reg: 0x4a,
                source: BusError::Disconnected,
            }
        ));
    }
}
