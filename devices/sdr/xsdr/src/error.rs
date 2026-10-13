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
    #[error("{chip} register {reg:#06x} {access} failed")]
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

    /// No chip answered where one must be. Not produced yet: no ported sequence reports
    /// it; it keeps `rsl-lowlevel`'s sequence failures mapped one to one.
    #[error("{0} not found")]
    ChipMissing(&'static str),

    /// A chip reported no input signal it needs. Not produced yet, as [`Self::ChipMissing`].
    #[error("{0} has no input signal")]
    InputMissing(&'static str),

    /// The hardware ID names no LMS7 family member (HWID bits 23:16), as libusdr's
    /// `xsdr_init` refuses it.
    #[error("unknown LMS7 family device ID {0:#04x}")]
    UnknownDevice(u8),

    /// A family member this driver identifies but does not bring up yet.
    #[error("{0:?} is not supported yet")]
    UnsupportedVariant(Variant),

    /// A sample rate outside libusdr's 90 kS/s to 195 MS/s (`-ERANGE`), or 0 for both
    /// directions (`-EINVAL`), refused before any bus access.
    #[error("sample rate {0} is out of range")]
    InvalidRate(u32),

    /// Gateware whose sample-rate path is not ported (HWID PHY bits, here): an RX or
    /// shared MMCM, or an FPGA DUC/DDC. Refused before any bus access.
    #[error("gateware PHY features {0:#04x} are not supported yet")]
    UnsupportedGateware(u8),

    /// The LMS7002M's processors cannot divide down to the rate: libusdr's `-EINVAL`
    /// ("can't deliver decimation").
    #[error("the LMS7002M cannot deliver this sample rate")]
    UndeliverableRate,

    /// The LMS7002M's clock generator locked on no candidate: libusdr's `-ERANGE`.
    #[error("the LMS7002M clock generator did not lock")]
    ClockUnlocked,

    /// The `LimeLight` link calibration found no clean clock phase, or (without a TX
    /// MMCM) measured none: libusdr's `-ERANGE`.
    #[error("the LimeLight link did not calibrate")]
    LinkCalibration,

    /// No VCO reaches the LO, or none locks: libusdr's `-ERANGE`.
    #[error("LO {0} Hz cannot be tuned")]
    TuneOutOfRange(u32),

    /// A VCO comparator read "fail" while tuning: libusdr's `-EIO`.
    #[error("a VCO comparator failed")]
    VcoComparatorFailed,

    /// A chip operation found no channel selected: libusdr's `-EINVAL`.
    #[error("no LMS7002M channel selected")]
    NoChannel,

    /// An operation needing a sample rate ran before one was set.
    #[error("no sample rate is set")]
    NoSampleRate,
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
