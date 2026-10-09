//! The driver's error type.

use std::fmt;

use crate::lowlevel::BusError;

/// A failed driver operation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A register access failed on the bus.
    #[error("{chip} register {reg:#04x} {access} failed")]
    #[non_exhaustive]
    Bus {
        /// The chip accessed: `"FPGA"` (gateware registers), `"LMS6002D"`, `"LP8758"`,
        /// `"Si5332"`, `"TMP114"` or `"TPS6381x"`.
        chip: &'static str,

        /// Whether it was a read or a write.
        access: Access,

        /// The register address, in the chip's own address space.
        reg: u32,

        /// The bus failure.
        #[source]
        source: BusError,
    },
    /// The board revision (HWID bits 15:8) is not one this driver supports.
    #[error("unsupported uSDR board revision {0}")]
    UnsupportedRevision(u8),
    /// A chip identified itself with an unexpected ID.
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
    /// The board is too hot for the thermal policy.
    #[error("board at {celsius:.1} °C, limit {limit:.1} °C")]
    #[non_exhaustive]
    Overheated {
        /// The reading that tripped the limit.
        celsius: f32,

        /// The limit it was compared with.
        limit: f32,
    },
    /// Custom thermal limits out of order or past the hard stop.
    #[error("invalid thermal limits: start {start}, stop {stop}, resume {resume}")]
    #[non_exhaustive]
    InvalidThermalLimits {
        /// Requested start limit.
        start: f32,

        /// Requested stop limit.
        stop: f32,

        /// Requested resume limit.
        resume: f32,
    },
    /// A sample rate the board cannot run at.
    #[error("unsupported sample rate {0} S/s")]
    UnsupportedSampleRate(u32),

    /// The Si5332 clock generator lost its input clock.
    #[error("Si5332 has no input clock")]
    ClockInputMissing,
}

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

/// Attaches the register access to a bus failure.
pub(crate) trait BusContext<T> {
    /// Names the register read that failed.
    fn reading(self, chip: &'static str, reg: impl Into<u32>) -> Result<T, Error>;

    /// Names the register write that failed.
    fn writing(self, chip: &'static str, reg: impl Into<u32>) -> Result<T, Error>;
}

impl<T> BusContext<T> for Result<T, BusError> {
    fn reading(self, chip: &'static str, reg: impl Into<u32>) -> Result<T, Error> {
        let reg = reg.into();
        self.map_err(|source| Error::Bus {
            chip,
            access: Access::Read,
            reg,
            source,
        })
    }

    fn writing(self, chip: &'static str, reg: impl Into<u32>) -> Result<T, Error> {
        let reg = reg.into();
        self.map_err(|source| Error::Bus {
            chip,
            access: Access::Write,
            reg,
            source,
        })
    }
}
