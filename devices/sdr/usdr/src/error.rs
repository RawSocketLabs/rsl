//! The driver's error type.

pub use rsl_lowlevel::Access;

use crate::frontend::ClockPlanError;
use crate::lowlevel::BusError;
use crate::portable::sequence::SequenceFailure;
use crate::portable::stream::RX_DMA;
use crate::stream::{HostFormat, RxChannels, StreamTime, SyncMode, WireFormat};

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
    #[error("stream transport failed")]
    Stream(#[source] BusError),

    /// No RX block arrived, or no TX buffer freed, within the timeout; or the RX DC
    /// estimator finished no measurement within libusdr's 8000 polls.
    #[error("timed out waiting for the gateware")]
    Timeout,

    /// The operation needs a sample rate, and none is set.
    #[error("no sample rate is set")]
    NoSampleRate,

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

    /// The call needs a stream group, or the group's stream it acts on (RX to receive,
    /// TX to send or mute): none was created, or it was stopped.
    #[error("no such stream exists")]
    NotStreaming,

    /// The call needs the stream group stopped first, or (starting it) not yet started.
    #[error("a stream group exists")]
    AlreadyStreaming,

    /// A stream configuration with no stream in it.
    #[error("no stream to create")]
    NoStreams,

    /// A device of a [`MultiDevice`](crate::MultiDevice) holds other streams than its
    /// master, as a failed create leaves them, so the aggregate cannot start them
    /// together.
    #[error("the device's streams differ from its aggregate master's")]
    UnmatchedStreams,

    /// A synchronisation mode libusdr's uSDR stream sync (`sfetrx4_stream_sync`) refuses.
    #[error("unsupported stream synchronisation {0:?}")]
    UnsupportedSync(SyncMode),

    /// The buffers' element type is not the stream's host format.
    #[error("buffers hold {buffer:?} samples, the stream's are {stream:?}")]
    #[non_exhaustive]
    HostFormatMismatch {
        /// The stream's host format.
        stream: HostFormat,

        /// The buffers' format.
        buffer: HostFormat,
    },

    /// A receive or send passed a different number of channel buffers than the stream has
    /// channels.
    #[error("{got} channel buffers for a stream of {expected} channels")]
    #[non_exhaustive]
    ChannelBuffers {
        /// The stream's channel count.
        expected: usize,

        /// The buffers passed.
        got: usize,
    },

    /// A packet size the stream engine cannot take, in samples: RX packets it cannot split
    /// into bursts, TX packets empty or larger than its FIFO.
    #[error("unsupported packet size {0} samples")]
    UnsupportedPacketSize(u32),

    /// A wire format the TX front end does not take; the uSDR's takes `ci16` only.
    #[error("unsupported TX wire format {0:?}")]
    UnsupportedWireFormat(WireFormat),

    /// A host format with no converter to or from the stream's wire format: a real host
    /// for a complex wire, or the reverse (where libusdr copies the bytes unconverted), or
    /// a real host for TX.
    #[error("no conversion between host format {host:?} and wire format {wire:?}")]
    #[non_exhaustive]
    NoConversion {
        /// The host format asked for.
        host: HostFormat,

        /// The wire format asked for.
        wire: WireFormat,
    },

    /// RX channels the receiver's front end does not carry in the wire format asked for:
    /// channel 1 complex (lanes 2 and 3).
    #[error("RX channels {channels:?} are not carried in wire format {wire:?}")]
    #[non_exhaustive]
    UnsupportedRxChannels {
        /// The channels asked for.
        channels: RxChannels,

        /// The wire format asked for.
        wire: WireFormat,
    },

    /// The gateware has the extended TX core (hardware ID bit 26), whose stream setup this
    /// driver does not port.
    #[error("the gateware's extended TX core is not supported")]
    UnsupportedTxCore,

    /// The gateware predates the TX stream engine libusdr requires (its
    /// `MINIM_FWID_COMPAT`, build 0xd2b10c09); the build ID as read.
    #[error("gateware build {0:#010x} is too old for TX streaming")]
    OutdatedGateware(u32),

    /// A send of no samples, or more than one packet holds.
    #[error("send of {got} samples; a TX packet holds 1 to {max}")]
    #[non_exhaustive]
    SendSize {
        /// Samples per packet.
        max: usize,

        /// Samples the buffers hold.
        got: usize,
    },

    /// A TX time past the core's 47-bit timestamp.
    #[error("TX time {0:?} exceeds the core's 47-bit timestamp")]
    UnsupportedTimestamp(StreamTime),

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

    /// No front end with a working oscillator DAC is attached.
    #[error("no front end with an oscillator DAC is attached")]
    NoOscillatorDac,

    /// The call needs a front end that is not attached; libusdr's name for it, or the names
    /// of the boards that would do.
    #[error("no {0} front end is attached")]
    FrontEndMissing(&'static str),

    /// A front end's clock synthesizer has no configuration for the requested frequencies
    /// (libusdr's `-EINVAL`).
    #[error("no clock plan for the requested frequencies")]
    ClockPlan(#[from] ClockPlanError),

    /// A front end's clock synthesizer did not report lock in time (libusdr's
    /// `-ETIMEDOUT`); which PLL, such as `"LMK05318 APLL1"`.
    #[error("{0} did not lock")]
    ClockUnlocked(&'static str),

    /// A register address the chip does not have, such as an LMS6002D address past its
    /// seven bits.
    #[error("{chip} has no register {reg:#04x}")]
    #[non_exhaustive]
    NoSuchRegister {
        /// The chip addressed.
        chip: &'static str,

        /// The address asked for.
        reg: u8,
    },

    /// The handle already has an operation in flight; nothing was sent for this one.
    #[error("another operation is in flight")]
    Busy,
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
            } => Self::bus(chip, access.context(), source),
            SequenceFailure::FpgaTransport {
                chip,
                access,
                source,
            } => Self::bus(chip, access.context(), source),
            SequenceFailure::ChipId {
                chip,
                expected,
                found,
            } => Self::ChipId {
                chip,
                expected,
                found,
            },
            // The stream engine's RX DMA reads dead as a missing chip reads absent.
            SequenceFailure::ChipMissing { chip: RX_DMA } => Self::DmaEngineFault,
            SequenceFailure::ChipMissing { chip } => Self::ChipMissing(chip),
            // Only the Si5332's clock sequences report a missing input.
            SequenceFailure::InputMissing { .. } => Self::ClockInputMissing,
        }
    }
}

/// A failed front-end probe or board access in `rsl-sdr-frontend`, as this driver reports
/// it.
impl From<rsl_sdr_frontend::Error> for Error {
    fn from(error: rsl_sdr_frontend::Error) -> Self {
        match error {
            rsl_sdr_frontend::Error::Bus {
                chip,
                access,
                reg,
                source,
                ..
            } => Self::Bus {
                chip,
                access,
                reg,
                source,
            },
            rsl_sdr_frontend::Error::Board { board, reason, .. } => {
                Self::FrontEnd { board, reason }
            }
            rsl_sdr_frontend::Error::NoOscillatorDac => Self::NoOscillatorDac,
        }
    }
}

impl Error {
    /// A failed register access, from the direction and register a sequence decoded.
    fn bus(chip: &'static str, (access, reg): (Access, u32), source: BusError) -> Self {
        Self::Bus {
            chip,
            access,
            reg,
            source,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lowlevel::{I2cAddr, SpiAddr};
    use crate::portable::sequence::{FpgaAccess, RegisterAccess};

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

    #[test]
    fn a_failed_spi_exchange_names_the_chip_register_and_direction() {
        let failure = SequenceFailure::FpgaTransport {
            chip: "LMS6002D",
            access: FpgaAccess::Spi {
                target: SpiAddr(0),
                word: 0x8412,
                reg: 4,
                write: true,
            },
            source: BusError::Timeout,
        };
        assert!(matches!(
            Error::from(failure),
            Error::Bus {
                chip: "LMS6002D",
                access: Access::Write,
                reg: 4,
                source: BusError::Timeout,
            }
        ));
    }

    #[test]
    fn a_missing_input_is_the_si5332_clock_input() {
        let failure = SequenceFailure::InputMissing { chip: "Si5332" };
        assert!(matches!(Error::from(failure), Error::ClockInputMissing));
    }

    #[test]
    fn a_dead_rx_dma_engine_is_a_dma_fault_and_other_missing_chips_stay_missing() {
        let dead = SequenceFailure::<BusError>::ChipMissing { chip: RX_DMA };
        assert!(matches!(Error::from(dead), Error::DmaEngineFault));
        let missing = SequenceFailure::<BusError>::ChipMissing { chip: "Si5332" };
        assert!(matches!(Error::from(missing), Error::ChipMissing("Si5332")));
    }
}
