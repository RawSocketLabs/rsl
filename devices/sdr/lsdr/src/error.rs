//! The driver's error type.

pub use hal::Access;
use hal::SequenceFailure;
use sdr_frontend::Board;

use crate::lowlevel::BusError;
use crate::portable::lmk04832::ClockPlanFailure;

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

    /// A chip identified itself with an unexpected ID: the Si549's device type, which the
    /// sample rate checks before tuning it (libusdr's `-EIO`).
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

    /// No chip answered where one must be: the RX DMA engine reading dead when a stream is
    /// created. An absent ADC or clock generator is not an error; it is reported by
    /// [`Device::fitted`](crate::Device::fitted).
    #[error("{0} not found")]
    #[non_exhaustive]
    ChipMissing(&'static str),

    /// A chip reported no input signal it needs. Not produced yet: no ported sequence checks
    /// one; it keeps `hal`'s sequence failures mapped one to one.
    #[error("{0} has no input signal")]
    #[non_exhaustive]
    InputMissing(&'static str),

    /// The front-end probe failed. Probing automatically, as open does, a board that fails
    /// is logged and skipped as libusdr skips it, so open reports none of these yet.
    #[error("front-end probe failed")]
    FrontEnd(#[from] sdr_frontend::Error),

    /// The call needs a front end that is not attached; libusdr's name for it.
    #[error("no {0} front end is attached")]
    FrontEndMissing(&'static str),

    /// A front-end board answered whose bring-up this driver does not port (fe4ch4007200);
    /// libusdr would bring it up. Open stops there.
    #[error("the {} front end answered; its bring-up is not ported", .0.name())]
    FrontEndNotPorted(Board),

    /// The Si549 refuses the ADC rate the sample rate implies (outside 200 kHz to 800 MHz,
    /// libusdr's `-ERANGE`), before any access.
    #[error("the {chip} cannot clock {rate} S/s")]
    #[non_exhaustive]
    UnsupportedSampleRate {
        /// The sample rate asked for, in S/s.
        rate: u32,

        /// The clock that refused it: `"Si549"`.
        chip: &'static str,
    },

    /// The LMK04832 has no plan for the sample rate, after the Si549 and the FIR gearbox
    /// are set, as libusdr fails there; the reason keeps libusdr's distinction
    /// (`-ERANGE` for no VCO, `-EINVAL` otherwise).
    #[error("the LMK04832 cannot distribute {rate} S/s")]
    #[non_exhaustive]
    ClockPlan {
        /// The sample rate asked for, in S/s.
        rate: u32,

        /// Why there is no plan.
        #[source]
        source: ClockPlanFailure,
    },

    /// The NCO was set before any sample rate (libusdr's `-EINVAL`, after it selected
    /// the channels).
    #[error("the NCO needs a sample rate first")]
    NoSampleRate,

    /// The RX stream's packet size cannot be split into the gateware's bursts (libusdr's
    /// `-EINVAL`, after the DMA reset).
    #[error("{0} samples per packet cannot be split into the RX front end's bursts")]
    UnsupportedPacketSize(u32),

    /// The RX stream cannot be synchronised: the `LSDr` declares no stream sync unit
    /// (`/ll/sync/0/base`), so libusdr's create fails there (`-ENOENT`), after configuring
    /// the RX front end and DMA, at the pinned revision.
    #[error("the LSDr declares no stream sync unit, so libusdr cannot create its RX stream")]
    NoSyncUnit,
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
