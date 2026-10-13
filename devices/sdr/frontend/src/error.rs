//! What a probe or a front-end access fails with, in terms each driver maps into its own
//! error.

use rsl_lowlevel::{Access, BusError, SequenceFailure};

/// A failed front-end probe or board access.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A register access failed on the bus.
    #[error("{chip} register {reg:#04x} {access} failed")]
    #[non_exhaustive]
    Bus {
        /// The chip accessed: `"FPGA"` (the connector's GPIO core and the SPI-extension
        /// configuration) or a front-end board's chip, such as `"TCA6416"`.
        chip: &'static str,

        /// Whether it was a read or a write.
        access: Access,

        /// The register address, in the chip's own address space.
        reg: u32,

        /// The bus failure.
        #[source]
        source: BusError,
    },

    /// A front-end board was named or answered, but did not come up.
    #[error("{board} front end failed: {reason}")]
    #[non_exhaustive]
    Board {
        /// libusdr's name for the board, such as `"pciefe"`.
        board: &'static str,

        /// What went wrong.
        reason: &'static str,
    },

    /// The front end has no oscillator DAC that answered (libusdr's `-EACCES` for a
    /// breakout whose DAC did not identify itself).
    #[error("no front end with an oscillator DAC is attached")]
    NoOscillatorDac,
}

impl Error {
    /// A failed register access.
    pub(crate) fn bus(
        chip: &'static str,
        access: Access,
        reg: impl Into<u32>,
        source: BusError,
    ) -> Self {
        Self::Bus {
            chip,
            access,
            reg: reg.into(),
            source,
        }
    }
}

/// A failed register-only sequence of this crate. Its machines only access registers and
/// SPI, so only transport failures arise; the other faults keep the chip they name.
impl From<SequenceFailure<BusError>> for Error {
    fn from(failure: SequenceFailure<BusError>) -> Self {
        match failure {
            SequenceFailure::Transport {
                chip,
                access,
                source,
            } => {
                let (access, reg) = access.context();
                Self::bus(chip, access, reg, source)
            }
            SequenceFailure::FpgaTransport {
                chip,
                access,
                source,
            } => {
                let (access, reg) = access.context();
                Self::bus(chip, access, reg, source)
            }
            SequenceFailure::ChipId { chip, .. } => Self::Board {
                board: chip,
                reason: "a chip reported an unexpected ID",
            },
            SequenceFailure::ChipMissing { chip } => Self::Board {
                board: chip,
                reason: "a chip did not answer",
            },
            SequenceFailure::InputMissing { chip } => Self::Board {
                board: chip,
                reason: "a chip has no input signal",
            },
        }
    }
}

/// Attaches the register access to a bus failure.
pub(crate) trait BusContext<T> {
    /// Names the register read that failed.
    fn reading(self, chip: &'static str, reg: impl Into<u32>) -> Result<T, Error>;

    /// Names the register write that failed.
    fn writing(self, chip: &'static str, reg: impl Into<u32>) -> Result<T, Error>;
}

impl<T> BusContext<T> for Result<T, BusError> {
    fn reading(self, chip: &'static str, reg: impl Into<u32>) -> Result<T, Error> {
        self.map_err(|source| Error::bus(chip, Access::Read, reg, source))
    }

    fn writing(self, chip: &'static str, reg: impl Into<u32>) -> Result<T, Error> {
        self.map_err(|source| Error::bus(chip, Access::Write, reg, source))
    }
}
