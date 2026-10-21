//! What a probe or a front-end access fails with, in terms each driver maps into its own
//! error.

use hal::{Access, BusError, SequenceFailure};
use lmk05318::driver::Error as Lmk05318Error;
use lmk05318::plan::ClockPlanError;

use crate::board::Board;

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

    /// A board's chip identified itself with an unexpected ID: XMASS's LMK05318, which
    /// libusdr's `lmk05318_create` refuses with `-ENODEV` (XMASS logs it and carries on).
    #[error("{chip} reports ID {found:#x}, expected {expected:#x}")]
    #[non_exhaustive]
    ChipId {
        /// Which chip.
        chip: &'static str,

        /// The ID the bring-up requires.
        expected: u32,

        /// The ID read.
        found: u32,
    },

    /// A board's clock synthesizer has no configuration for the requested frequencies
    /// (libusdr's `-EINVAL`).
    #[error("no clock plan for the requested frequencies")]
    ClockPlan(#[from] ClockPlanError),

    /// A board's clock synthesizer did not report lock in time (libusdr's `-ETIMEDOUT`);
    /// which PLL, such as `"LMK05318 APLL1"`.
    #[error("{0} did not lock")]
    ClockUnlocked(&'static str),

    /// A board refuses a setting, as libusdr does with `-EINVAL`.
    #[error("{board} front end refuses the setting: {reason}")]
    #[non_exhaustive]
    InvalidSetting {
        /// libusdr's name for the board, such as `"fe1005000"`.
        board: &'static str,

        /// Why.
        reason: &'static str,
    },

    /// A board answered whose bring-up this crate does not port; libusdr would bring it up.
    /// The probe stops there, hinted or not.
    #[error("the {} front end answered; its bring-up is not ported", .0.name())]
    NotPorted(Board),
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

/// A failed LMK05318 operation, named as the boards' other chips are.
impl From<Lmk05318Error> for Error {
    fn from(error: Lmk05318Error) -> Self {
        match error {
            Lmk05318Error::Bus {
                access,
                reg,
                source,
            } => Self::bus(lmk05318::NAME, access, reg, source),
            Lmk05318Error::ChipId { expected, found } => Self::ChipId {
                chip: lmk05318::NAME,
                expected,
                found,
            },
            Lmk05318Error::ClockPlan(error) => Self::ClockPlan(error),
            Lmk05318Error::Unlocked(pll) => Self::ClockUnlocked(pll),
        }
    }
}

/// A failed register-only sequence of this crate. Its machines (`GpioSetup`,
/// `Fe1005000Probe`) only access registers and SPI, so only transport failures arise; the
/// chip faults no machine of this crate reports are kept as a front-end failure, without
/// naming the chip as a board.
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
            SequenceFailure::ChipId { .. } => Self::Board {
                board: "front end",
                reason: "a chip reported an unexpected ID",
            },
            SequenceFailure::ChipMissing { .. } => Self::Board {
                board: "front end",
                reason: "a chip did not answer",
            },
            SequenceFailure::InputMissing { .. } => Self::Board {
                board: "front end",
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
