//! The driver's error type.

use std::fmt;

use crate::lowlevel::BusError;
use crate::portable::sequence::{FpgaAccess, RegisterAccess, SequenceFailure};
use crate::stream::{HostFormat, SyncMode};

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

    /// A frequency the board cannot tune to.
    #[error("unsupported frequency {0} Hz")]
    UnsupportedFrequency(u32),

    /// An external reference the Si5332's PLL cannot use, in Hz.
    #[error("unsupported reference clock {0} Hz")]
    UnsupportedReferenceClock(u32),

    /// The LMS6002D's RX synthesizer found no capacitor code that locks at this LO, in Hz.
    #[error("RX PLL cannot lock at {0} Hz")]
    PllUnlocked(u32),

    /// The LMS6002D's synthesizer read back something it cannot have: a divider other than
    /// the one written, or both tuning-voltage comparator bits at once. A failed divider
    /// read counts as a wrong one, as in libusdr.
    #[error("LMS6002D synthesizer read back an impossible value")]
    PllFault,

    /// The transport could not open the device.
    #[error("cannot open the device")]
    Open(#[source] BusError),

    /// A stream operation failed in the transport (opening, waiting for or releasing a
    /// DMA block, closing).
    #[error("RX stream transport failed")]
    Stream(#[source] BusError),

    /// No RX block arrived within the timeout.
    #[error("timed out waiting for RX samples")]
    Timeout,

    /// The receive buffer is smaller than one packet.
    #[error("receive buffer holds {got} samples, a packet has {needed}")]
    #[non_exhaustive]
    BufferTooSmall {
        /// Samples in a packet.
        needed: usize,

        /// Samples the buffer holds.
        got: usize,
    },

    /// The transport delivered an RX block shorter than the stream's, in bytes; the
    /// buffer is untouched.
    #[error("RX block of {got} bytes, the stream's is {expected}")]
    #[non_exhaustive]
    ShortRxBlock {
        /// The stream's block size.
        expected: usize,

        /// What the transport delivered.
        got: usize,
    },

    /// The call needs a stream: none was created, or it was stopped.
    #[error("no stream group exists")]
    NotStreaming,

    /// The call needs the stream group stopped first, or (starting it) not yet started.
    #[error("a stream group exists")]
    AlreadyStreaming,

    /// A stream configuration with no stream in it.
    #[error("no stream to create")]
    NoStreams,

    /// A synchronisation mode libusdr's uSDR stream sync (`sfetrx4_stream_sync`) refuses.
    #[error("unsupported stream synchronisation {0:?}")]
    UnsupportedSync(SyncMode),

    /// The receive buffers' element type is not the stream's host format.
    #[error("receive buffers hold {buffer:?} samples, the stream delivers {stream:?}")]
    #[non_exhaustive]
    HostFormatMismatch {
        /// The stream's host format.
        stream: HostFormat,

        /// The buffers' format.
        buffer: HostFormat,
    },

    /// The receive call passed a different number of channel buffers than the stream has
    /// channels.
    #[error("{got} channel buffers for a stream of {expected} channels")]
    #[non_exhaustive]
    ChannelBuffers {
        /// The stream's channel count.
        expected: usize,

        /// The buffers passed.
        got: usize,
    },

    /// A packet size the stream engine cannot split into bursts, in samples.
    #[error("unsupported packet size {0} samples")]
    UnsupportedPacketSize(u32),

    /// The FPGA's RX DMA engine reads as dead (all status bits set).
    #[error("FPGA RX DMA engine is not responding")]
    DmaEngineFault,

    /// The Si5332 clock generator lost its input clock.
    #[error("Si5332 has no input clock")]
    ClockInputMissing,

    /// A front-end board was selected or answered, but did not come up.
    #[error("{board} front end failed: {reason}")]
    #[non_exhaustive]
    FrontEnd {
        /// libusdr's name for the board, such as `"pciefe"`.
        board: &'static str,

        /// What went wrong.
        reason: &'static str,
    },

    /// A front-end board answered whose bring-up this driver does not port; libusdr's
    /// name for it. Opening with
    /// [`FrontEndSelection::None`](crate::FrontEndSelection::None) skips the probe.
    #[error("{0} front end detected; its bring-up is not supported")]
    UnsupportedFrontEnd(&'static str),

    /// No front end with a working oscillator DAC is attached.
    #[error("no front end with an oscillator DAC is attached")]
    NoOscillatorDac,

    /// The call needs a front end that is not attached; libusdr's name for it.
    #[error("no {0} front end is attached")]
    FrontEndMissing(&'static str),
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

/// A failed portable register sequence, with the register or chip it names.
impl From<SequenceFailure<BusError>> for Error {
    fn from(failure: SequenceFailure<BusError>) -> Self {
        match failure {
            SequenceFailure::Transport {
                chip,
                access,
                source,
            } => {
                let (access, reg) = match access {
                    RegisterAccess::Read { reg, .. } => (Access::Read, reg),
                    RegisterAccess::Write { reg, .. } => (Access::Write, reg),
                };
                Self::Bus {
                    chip,
                    access,
                    reg: reg.into(),
                    source,
                }
            }
            SequenceFailure::FpgaTransport {
                chip,
                access,
                source,
            } => {
                let (access, reg) = match access {
                    FpgaAccess::Read { reg } => (Access::Read, reg.into()),
                    FpgaAccess::Write { reg, .. } => (Access::Write, reg.into()),
                    FpgaAccess::Spi { reg, write, .. } => {
                        (if write { Access::Write } else { Access::Read }, reg.into())
                    }
                };
                Self::Bus {
                    chip,
                    access,
                    reg,
                    source,
                }
            }
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
            SequenceFailure::ClockInputMissing => Self::ClockInputMissing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lowlevel::I2cAddr;

    #[test]
    fn a_failed_sequence_read_keeps_its_chip_register_and_direction() {
        let failure = SequenceFailure::Transport {
            chip: "TPS6381x",
            access: RegisterAccess::Read {
                dev: I2cAddr::new(0, 0x75),
                reg: 0x03,
            },
            source: BusError::Disconnected,
        };
        assert!(matches!(
            Error::from(failure),
            Error::Bus {
                chip: "TPS6381x",
                access: Access::Read,
                reg: 0x03,
                source: BusError::Disconnected,
            }
        ));
    }

    #[test]
    fn a_failed_sequence_write_is_reported_as_a_write() {
        let failure = SequenceFailure::Transport {
            chip: "TPS6381x",
            access: RegisterAccess::Write {
                dev: I2cAddr::new(0, 0x75),
                reg: 0x01,
                value: 0x28,
            },
            source: BusError::Disconnected,
        };
        assert!(matches!(
            Error::from(failure),
            Error::Bus {
                access: Access::Write,
                reg: 0x01,
                ..
            }
        ));
    }
}
