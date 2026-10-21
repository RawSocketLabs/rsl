//! A stream's setup: what libusdr's `create_limesdr_stream` (`ipblks/streams/
//! stream_limesdr.c`) decides before it asks the transport for the stream, its format's
//! converter (`sdr-formats`, libusdr's `get_transform_fn`) and its burst geometry.
//!
//! The FT601 transport that would carry the stream is not in the pinned libusdr, so no
//! stream runs yet: [`Device::setup_stream`](crate::Device::setup_stream) returns the setup
//! after preparing the LMS7002M, where libusdr hands it to the transport.

use lms7002m::ctrl::Direction;
use sdr_formats::{Converter, SampleFormat};

use crate::error::Error;

/// Bytes in one transport burst, its header included (`burst_size`).
const BURST_BYTES: u32 = 4096;

/// Bytes of a burst's header (`LIMESDR_STREAM_HDR`): flags, the payload size and the time.
pub const BURST_HEADER_BYTES: u32 = 16;

/// Transport buffers libusdr asks for (`buffer_count`).
const BUFFERS: u32 = 16;

/// Samples per packet libusdr takes for a TX packet of 0 (`32 * 1020`).
const TX_DEFAULT_SAMPLES: u32 = 32 * 1020;

/// The fewest samples in a TX packet; smaller ones are raised to it.
const TX_MIN_SAMPLES: u32 = 4096;

/// A sample format the `LimeSDR`'s gateware packs in a burst, little-endian, I then Q
/// (`get_bits_fmt`'s 12 and 16 bits; the stream takes complex samples only).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WireFormat {
    /// 16 bits per value, 4 bytes per sample (`ci16`).
    #[default]
    Ci16,

    /// 12 bits per value, packed back to back, 3 bytes per sample (`ci12`).
    Ci12,
}

impl WireFormat {
    /// Bits one sample, I and Q, takes.
    const fn sample_bits(self) -> u32 {
        match self {
            Self::Ci16 => 32,
            Self::Ci12 => 24,
        }
    }

    /// The same format as libusdr's converters name it.
    const fn sample_format(self) -> SampleFormat {
        match self {
            Self::Ci16 => SampleFormat::Ci16,
            Self::Ci12 => SampleFormat::Ci12,
        }
    }

    /// TX sends a whole number of these samples (`tx_sampl_c`: 16 bytes of `ci16`, 48 of
    /// `ci12`).
    const fn tx_sample_multiple(self) -> u32 {
        match self {
            Self::Ci16 => 4,
            Self::Ci12 => 16,
        }
    }
}

/// One stream's setup on one channel, as `create_limesdr_stream` computes it: the
/// converter between the host's buffers and the wire, and the bursts each transport buffer
/// holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamSetup {
    /// The direction.
    direction: Direction,

    /// Samples per packet, after TX's defaults (`host_pkt_symbs`).
    samples_per_packet: u32,

    /// Bursts per transport buffer (`burst_count`).
    bursts: u32,

    /// Samples per burst (`burst_symbs`).
    samples_per_burst: u32,

    /// Host bytes of a burst's samples (`burst_host_bytes`).
    host_bytes_per_burst: u32,

    /// TX sends a whole number of these samples; 0 for RX (`tx_sampl_c`).
    sample_multiple: u32,

    /// Wire to host (RX) or host to wire (TX) conversion (`tf_data`).
    converter: Converter,
}

impl StreamSetup {
    /// The setup of a `direction` stream of `samples_per_packet` samples per packet,
    /// received into or sent from `host` buffers over a `wire` burst payload
    /// (`create_limesdr_stream` with `host@wire`, one channel).
    ///
    /// As libusdr: a burst is 4096 bytes, its 16-byte header and as many whole samples as
    /// the rest holds (1020 in `ci16`, 1360 in `ci12`); an RX packet that is not a whole
    /// number of bursts takes 0.4 of a burst per burst's worth of samples, at least one
    /// burst; a TX packet of 0 samples is 32 640, one under 4096 is 4096, and a TX
    /// buffer holds the bursts the packet fills. The arithmetic is exact where libusdr's
    /// 32-bit doubling of an RX packet past 2^31 samples wraps (a divergence no board
    /// reaches).
    ///
    /// # Errors
    ///
    /// [`Error::UnsupportedFormat`] for a host with no converter to or from `wire` (libusdr
    /// byte-copies a pair it does not know, such as a real host); a divergence.
    /// [`Error::UnsupportedPacketSize`] for a TX packet that is not a whole number of the
    /// wire's sample multiple (4 in `ci16`, 16 in `ci12`), as libusdr's `-EINVAL`, and for
    /// a packet whose transport buffer passes 32 bits, which libusdr wraps (a divergence).
    ///
    /// # Panics
    ///
    /// Never: a burst's payload and its host bytes are a few KiB.
    pub fn new(
        direction: Direction,
        wire: WireFormat,
        host: SampleFormat,
        samples_per_packet: u32,
    ) -> Result<Self, Error> {
        let converter = match direction {
            Direction::Rx => Converter::new(wire.sample_format(), host, 1, 1),
            Direction::Tx => Converter::new(host, wire.sample_format(), 1, 1),
        }
        .map_err(|_| Error::UnsupportedFormat { wire, host })?;
        let samples_per_burst = (BURST_BYTES - BURST_HEADER_BYTES) * 8 / wire.sample_bits();
        let (samples, bursts, sample_multiple) = match direction {
            Direction::Rx => {
                let samples = u64::from(samples_per_packet);
                let per_burst = u64::from(samples_per_burst);
                let bursts = if samples % per_burst == 0 {
                    samples / per_burst
                } else {
                    2 * samples / per_burst / 5
                };
                (samples_per_packet, bursts, 0)
            }
            Direction::Tx => {
                let samples = match samples_per_packet {
                    0 => TX_DEFAULT_SAMPLES,
                    samples => samples.max(TX_MIN_SAMPLES),
                };
                let multiple = wire.tx_sample_multiple();
                if samples % multiple != 0 {
                    return Err(Error::UnsupportedPacketSize(samples_per_packet));
                }
                (
                    samples,
                    u64::from(samples.div_ceil(samples_per_burst)),
                    multiple,
                )
            }
        };
        let bursts = u32::try_from(bursts.max(1))
            .ok()
            .filter(|bursts| bursts.checked_mul(BURST_BYTES).is_some())
            .ok_or(Error::UnsupportedPacketSize(samples_per_packet))?;
        let payload = usize::try_from(BURST_BYTES - BURST_HEADER_BYTES)
            .expect("invariant: a burst fits in usize");
        let host_bytes = match direction {
            Direction::Rx => converter.output_bytes(payload),
            Direction::Tx => converter.input_bytes(payload),
        };
        Ok(Self {
            direction,
            samples_per_packet: samples,
            bursts,
            samples_per_burst,
            host_bytes_per_burst: u32::try_from(host_bytes)
                .expect("invariant: at most twice a burst's payload"),
            sample_multiple,
            converter,
        })
    }

    /// The direction.
    #[must_use]
    pub const fn direction(&self) -> Direction {
        self.direction
    }

    /// Samples per packet: the one asked for, or on TX its default or minimum.
    #[must_use]
    pub const fn samples_per_packet(&self) -> u32 {
        self.samples_per_packet
    }

    /// Bursts per transport buffer.
    #[must_use]
    pub const fn bursts(&self) -> u32 {
        self.bursts
    }

    /// Samples per burst.
    #[must_use]
    pub const fn samples_per_burst(&self) -> u32 {
        self.samples_per_burst
    }

    /// Host bytes of one burst's samples.
    #[must_use]
    pub const fn host_bytes_per_burst(&self) -> u32 {
        self.host_bytes_per_burst
    }

    /// Bytes per transport buffer, the bursts' headers included (`block_size`).
    #[must_use]
    pub const fn block_bytes(&self) -> u32 {
        self.bursts * BURST_BYTES
    }

    /// Transport buffers the stream asks for.
    #[must_use]
    pub const fn buffers(&self) -> u32 {
        BUFFERS
    }

    /// The sample count every TX send is a whole number of; 0 on RX.
    #[must_use]
    pub const fn sample_multiple(&self) -> u32 {
        self.sample_multiple
    }

    /// The conversion between the host's buffers and a burst's payload.
    #[must_use]
    pub const fn converter(&self) -> &Converter {
        &self.converter
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rx_packets_of_whole_bursts_take_one_buffer_burst_each() {
        let setup = StreamSetup::new(Direction::Rx, WireFormat::Ci16, SampleFormat::Cf32, 2040)
            .expect("ported");
        assert_eq!((setup.bursts(), setup.block_bytes()), (2, 8192));
        assert_eq!(setup.host_bytes_per_burst(), 8160, "1020 cf32 samples");
        // 2000 samples are not whole bursts: 2 * 2000 / 1020 / 5, at least one.
        let setup = StreamSetup::new(Direction::Rx, WireFormat::Ci16, SampleFormat::Ci16, 2000)
            .expect("ported");
        assert_eq!(setup.bursts(), 1);
    }

    #[test]
    fn tx_packets_take_libusdrs_defaults_and_multiples() {
        let setup = StreamSetup::new(Direction::Tx, WireFormat::Ci12, SampleFormat::Ci16, 0)
            .expect("ported");
        assert_eq!(setup.samples_per_packet(), 32_640);
        assert_eq!(setup.bursts(), 24, "32 640 over 1360 a burst");
        let setup = StreamSetup::new(Direction::Tx, WireFormat::Ci16, SampleFormat::Ci16, 100)
            .expect("ported");
        assert_eq!(setup.samples_per_packet(), 4096);
        assert!(matches!(
            StreamSetup::new(Direction::Tx, WireFormat::Ci12, SampleFormat::Ci16, 4104),
            Err(Error::UnsupportedPacketSize(4104))
        ));
    }

    #[test]
    fn buffers_past_32_bits_and_unconvertible_hosts_are_refused() {
        assert!(matches!(
            StreamSetup::new(
                Direction::Rx,
                WireFormat::Ci16,
                SampleFormat::Ci16,
                u32::MAX - 39
            ),
            Err(Error::UnsupportedPacketSize(_))
        ));
        assert!(matches!(
            StreamSetup::new(Direction::Rx, WireFormat::Ci16, SampleFormat::F32, 1020),
            Err(Error::UnsupportedFormat { .. })
        ));
    }
}
