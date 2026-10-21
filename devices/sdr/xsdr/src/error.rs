//! The driver's error type.

pub use hal::Access;
use hal::SequenceFailure;

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

    /// No chip answered where one must be: the RX DMA engine's status reading dead
    /// (libusdr's `-EIO`, "DMA engine is dead").
    #[error("{0} not found")]
    ChipMissing(&'static str),

    /// A chip reported no input signal it needs. Not produced yet, as [`Self::ChipMissing`].
    #[error("{0} has no input signal")]
    InputMissing(&'static str),

    /// The hardware ID names no LMS7 family member (HWID bits 23:16), as libusdr's
    /// `xsdr_init` refuses it.
    #[error("unknown LMS7 family device ID {0:#04x}")]
    UnknownDevice(u8),

    /// A family member this driver identifies but does not bring up yet: the sSDR and
    /// sSDR Pro.
    #[error("{0:?} is not supported yet")]
    UnsupportedVariant(Variant),

    /// A sample rate outside libusdr's 90 kS/s to 195 MS/s (`-ERANGE`), or 0 for both
    /// directions (`-EINVAL`), refused before any bus access.
    #[error("sample rate {0} is out of range")]
    InvalidRate(u32),

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

    /// A path the board has no band for, or a TX path given as an RF path's RX selection:
    /// libusdr's `-EINVAL`.
    #[error("the board has no band for this path")]
    InvalidPath,

    /// A baseband NCO frequency beyond half the converter clock: libusdr's `-EINVAL`.
    #[error("NCO frequency {0} Hz is beyond half the converter clock")]
    NcoOutOfRange(i32),

    /// A DC offset outside the 11-bit DACs' −1024 to 1023, an I/Q gain correction above
    /// 2047 or a phase correction outside −2048 to 2047, which libusdr masks into the
    /// register: refused before any bus access.
    #[error("correction value out of range")]
    InvalidCorrection,

    /// A stream already exists: libusdr's `-EBUSY`.
    #[error("a stream already exists")]
    AlreadyStreaming,

    /// No stream exists to start, read or stop.
    #[error("no stream exists")]
    NotStreaming,

    /// Both RX channels asked of a double-pumped link, which carries one: libusdr's
    /// `-EINVAL` ("In double pump mode only SISO mode is allowed").
    #[error("a double-pumped link streams one channel")]
    DoublePumpedMimo,

    /// No burst split carries packets of this many samples: libusdr's `-EINVAL`.
    #[error("{0} samples per packet cannot be streamed")]
    UnsupportedPacketSize(u32),

    /// The transport failed to set up, deliver or release stream buffers.
    #[error("stream transport failed")]
    Stream(#[source] BusError),

    /// No packet arrived in time: libusdr's `-ETIMEDOUT`.
    #[error("no packet arrived in time")]
    Timeout,

    /// The transport delivered a block shorter than a packet.
    #[error("RX block of {got} bytes, expected {expected}")]
    #[non_exhaustive]
    ShortRxBlock {
        /// The packet's bytes.
        expected: usize,

        /// The block's.
        got: usize,
    },

    /// A receive given other than one buffer per channel of the stream.
    #[error("{got} buffers for a stream of {expected} channels")]
    #[non_exhaustive]
    ChannelBuffers {
        /// The stream's channels.
        expected: usize,

        /// The buffers given.
        got: usize,
    },

    /// A receive buffer shorter than a packet.
    #[error("buffer of {got} samples, a packet is {needed}")]
    #[non_exhaustive]
    BufferTooSmall {
        /// The packet's samples.
        needed: usize,

        /// The buffer's.
        got: usize,
    },

    /// The TX MMCM, which clocks the `LimeLight` link's FPGA side, did not lock: libusdr's
    /// `-EIO` ("MMCM NO CLOCK", "MMCM Ready flag timed out").
    #[error("the TX MMCM did not lock")]
    TxClockUnlocked,

    /// A link the TX PHY's MMCM cannot be configured for, refused before any bus access:
    /// with TX running, TX sample rates below about 6.35 MS/s, and on gateware with an RX
    /// or shared MMCM RX rates likewise, for which libusdr reads past its 64-entry lock
    /// and filter table (`mmcm_rom`).
    #[error("the TX PHY's MMCM cannot be configured for this link")]
    TxClockOutOfRange,

    /// Gateware built before the oldest libusdr streams TX from (`MINIM_FWID_COMPAT`):
    /// libusdr's `-ECONNRESET`. The build stamp (`USR_ACCESS2`) is given.
    #[error("gateware {0:#010x} is too old for TX streaming")]
    OutdatedGateware(u32),

    /// A send of channel buffers of different lengths.
    #[error("channel buffers of {first} and {other} samples")]
    #[non_exhaustive]
    UnequalChannelBuffers {
        /// The first buffer's samples.
        first: usize,

        /// Another's.
        other: usize,
    },

    /// A send of no samples or more than a packet.
    #[error("a send of {got} samples, a packet is at most {max}")]
    #[non_exhaustive]
    SendSize {
        /// The packet's samples.
        max: usize,

        /// The send's.
        got: usize,
    },

    /// A word of the RX PHY's capture read busy (bits 31:24) on every one of the quality
    /// check's 1000 reads; libusdr reads on forever.
    #[error("the RX PHY capture stayed busy")]
    CaptureBusy,

    /// A `LimeLight` clock-phase override above 512, whose delay no MMCM output holds,
    /// refused before any state change.
    #[error("phase override {0} is past 512")]
    InvalidPhase(u16),

    /// An RX PHY delay above 31, which libusdr masks to five bits, refused before any bus
    /// access.
    #[error("RX PHY delay {0} is past 31")]
    InvalidDelay(u8),

    /// A TX time past the TX core's 47 bits.
    #[error("TX time {0} is past the TX core's 47 bits")]
    UnsupportedTimestamp(u64),

    /// A front end failed: the hinted board's bring-up at open (probing automatically, a
    /// board that fails is logged and skipped as libusdr skips it), or one of its
    /// controls.
    #[error("front end failed")]
    FrontEnd(#[from] sdr_frontend::Error),

    /// An open's `fe=` hint, or `fe=none`, found no front end: libusdr's `-ENODEV`, which
    /// fails the XSDR's open.
    #[error("the front-end hint found no front end")]
    NoFrontEnd,

    /// No oscillator trim DAC to set: an XSDR without the `PCIe` front end or the
    /// M.2-to-`PCIe` breakout (libusdr's `-ENOTSUP`).
    #[error("no oscillator trim DAC is fitted")]
    NoOscillatorDac,

    /// The call needs a front end that is not attached: libusdr's name for it, or the
    /// names of the boards that would do.
    #[error("no {0} front end is attached")]
    FrontEndMissing(&'static str),
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
