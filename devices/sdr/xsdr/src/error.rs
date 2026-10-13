//! The driver's error type.

pub use rsl_lowlevel::Access;
use rsl_lowlevel::SequenceFailure;

use crate::lowlevel::BusError;
use crate::portable::hwid::Variant;

/// A failed driver operation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A register access failed on the bus.
    #[error("{chip} register {reg:#04x} {access} failed")]
    #[non_exhaustive]
    Bus {
        /// The chip accessed: `"FPGA"` (gateware registers), `"LMS7002M"` or `"LP8758"`.
        chip: &'static str,

        /// Whether it was a read or a write.
        access: Access,

        /// The register address, in the chip's own address space.
        reg: u32,

        /// The bus failure.
        #[source]
        source: BusError,
    },

    /// A chip identified itself with an unexpected ID: the LP8758's revision word
    /// (`OTP_REV << 8 | DEV_REV`), or the LMS7002M's version and revision (bits 15:6 of
    /// its `0x002F`).
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

    /// No chip answered where one must be.
    #[error("{0} not found")]
    ChipMissing(&'static str),

    /// A chip reported no input signal it needs.
    #[error("{0} has no input signal")]
    InputMissing(&'static str),

    /// The hardware ID names no LMS7 family member (HWID bits 23:16), as libusdr's
    /// `xsdr_init` refuses it.
    #[error("unknown LMS7 family device ID {0:#04x}")]
    UnknownDevice(u8),

    /// A family member this driver identifies but does not bring up yet.
    #[error("{0:?} is not supported yet")]
    UnsupportedVariant(Variant),
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
    fn a_failed_spi_exchange_names_the_lms7002m_register_not_the_word() {
        let failure = SequenceFailure::FpgaTransport {
            chip: "LMS7002M",
            access: FpgaAccess::Spi {
                target: SpiAddr(0),
                word: 0x8082_003e,
                reg: 0x0082,
                write: true,
            },
            source: BusError::Disconnected,
        };
        assert!(matches!(
            Error::from(failure),
            Error::Bus {
                chip: "LMS7002M",
                access: Access::Write,
                reg: 0x0082,
                source: BusError::Disconnected,
            }
        ));
    }
}
