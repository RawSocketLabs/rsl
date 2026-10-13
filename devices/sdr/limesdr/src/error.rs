//! The driver's error types.

pub use hal::Access;
use hal::SequenceFailure;

use crate::control::BusError;
use crate::portable::open::Failure;

/// A failed LMS64C exchange.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ExchangeError {
    /// The transport failed.
    #[error(transparent)]
    Bus(#[from] BusError),

    /// The firmware did not complete the command: the reply's status, one of
    /// `lms64c_cmds.h`'s `STATUS_*` other than `STATUS_COMPLETED_CMD` (1).
    #[error("the board firmware refused the command: status {0}")]
    Refused(u8),

    /// The reply answers another command than the request: the command it names.
    #[error("the board replied to command {0:#04x}, not the one sent")]
    Mismatched(u8),
}

/// A failed driver operation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A register access failed on the control channel.
    #[error("{chip} register {reg:#06x} {access} failed")]
    #[non_exhaustive]
    Bus {
        /// The chip accessed: `"FPGA"` (gateware registers) or `"LMS7002M"`.
        chip: &'static str,

        /// Whether it was a read or a write.
        access: Access,

        /// The register address, in the chip's own address space.
        reg: u32,

        /// The exchange's failure.
        #[source]
        source: ExchangeError,
    },

    /// The LMS7002M's reset pulse failed.
    #[error("LMS7002M reset failed")]
    #[non_exhaustive]
    Reset {
        /// The exchange's failure.
        #[source]
        source: ExchangeError,
    },

    /// A chip identified itself with an unexpected ID: the LMS7002M's version word, which
    /// must be exactly 0x3841 on this board.
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

    /// The reference-clock frequency detector did not finish within libusdr's ten polls,
    /// 50 ms apart.
    #[error("the reference-clock detector did not finish")]
    DetectorStalled,

    /// The reference clock measured no nominal reference (10, 30.72, 38.4, 40 or 52 MHz)
    /// within 1 MHz.
    #[error("unrecognised reference clock: measured {measured_hz} Hz")]
    #[non_exhaustive]
    UnknownReference {
        /// The measurement, in hertz.
        measured_hz: u64,
    },

    /// A sample rate outside libusdr's 90 kS/s to 122.88 MS/s (`-ERANGE`), or 0 for both
    /// directions (`-EINVAL`), refused before any exchange.
    #[error("sample rate {0} is out of range")]
    InvalidRate(u32),

    /// The LMS7002M's processors cannot divide down to the rate: libusdr's `-EINVAL`
    /// ("can't deliver decimation").
    #[error("the LMS7002M cannot deliver this sample rate")]
    UndeliverableRate,

    /// The LMS7002M's clock generator locked on no candidate: libusdr's `-ERANGE`.
    #[error("the LMS7002M clock generator did not lock")]
    ClockUnlocked,

    /// No VCO reaches the LO, or none locks: libusdr's `-ERANGE`.
    #[error("LO {0} Hz cannot be tuned")]
    TuneOutOfRange(u32),

    /// A VCO comparator read "fail" while tuning: libusdr's `-EIO`.
    #[error("a VCO comparator failed")]
    VcoComparatorFailed,

    /// Streaming was prepared with no sample rate: the default one did not lock.
    #[error("no sample rate is set")]
    NoSampleRate,
}

/// A failed open, with the register or chip it names.
impl From<Failure<ExchangeError>> for Error {
    fn from(failure: Failure<ExchangeError>) -> Self {
        match failure {
            Failure::Sequence(failure) => failure.into(),
            Failure::Reset(source) => Self::Reset { source },
            Failure::DetectorStalled => Self::DetectorStalled,
            Failure::UnknownReference { measured_hz } => Self::UnknownReference { measured_hz },
        }
    }
}

/// A failed portable register sequence, with the register or chip it names.
impl From<SequenceFailure<ExchangeError>> for Error {
    fn from(failure: SequenceFailure<ExchangeError>) -> Self {
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
    fn a_failed_spi_exchange_names_the_lms7002m_register_not_the_word() {
        let failure = SequenceFailure::FpgaTransport {
            chip: "LMS7002M",
            access: FpgaAccess::Spi {
                target: SpiAddr(0),
                word: 0x002f_0000,
                reg: 0x002f,
                write: false,
            },
            source: ExchangeError::Refused(5),
        };
        assert!(matches!(
            Error::from(Failure::Sequence(failure)),
            Error::Bus {
                chip: "LMS7002M",
                access: Access::Read,
                reg: 0x002f,
                source: ExchangeError::Refused(5),
            }
        ));
    }
}
