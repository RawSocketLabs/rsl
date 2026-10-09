//! The driver's error type.

use crate::lowlevel::BusError;

/// A failed driver operation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A bus transaction failed.
    #[error("{op} failed")]
    #[non_exhaustive]
    Bus {
        /// What the driver was doing.
        op: &'static str,

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
    /// The Si5332 clock generator lost its input clock.
    #[error("Si5332 has no input clock")]
    ClockInputMissing,
}

/// Attaches the driver operation to a bus failure.
pub(crate) trait BusContext<T> {
    /// Names the operation that failed.
    fn during(self, op: &'static str) -> Result<T, Error>;
}

impl<T> BusContext<T> for Result<T, BusError> {
    fn during(self, op: &'static str) -> Result<T, Error> {
        self.map_err(|source| Error::Bus { op, source })
    }
}
