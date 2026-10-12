//! The stream group's public configuration and results: what [`Device::create_streams`]
//! takes and what [`Device::receive_into`] reports.
//!
//! The uSDR's streams form one device-owned group: the sync unit they share is per device,
//! so they are created, synchronised, started and stopped together. libusdr's names:
//! `usdr_dms_create` (with its `"host@wire"` format string), `usdr_dms_sync`,
//! `usdr_dms_op` and `usdr_dms_recv`. Only RX is ported; TX will join the same group.
//!
//! [`Device::create_streams`]: crate::Device::create_streams
//! [`Device::receive_into`]: crate::Device::receive_into

use num_complex::Complex;

use crate::dsp::SampleFormat;

/// The streams to create and how to start them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct StreamsConfig {
    /// The RX stream, if any. A group needs at least one stream.
    pub rx: Option<RxConfig>,

    /// How the group starts.
    pub sync: SyncMode,
}

impl StreamsConfig {
    /// A group of one RX stream, started unsynchronised ([`SyncMode::None`]).
    #[must_use]
    pub const fn rx(rx: RxConfig) -> Self {
        Self {
            rx: Some(rx),
            sync: SyncMode::None,
        }
    }

    /// This configuration, started with `sync` instead.
    #[must_use]
    pub const fn with_sync(mut self, sync: SyncMode) -> Self {
        self.sync = sync;
        self
    }
}

/// An RX stream: its packet size, the format the gateware sends and the format the host
/// receives, converted from it inside the transport's buffer loan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RxConfig {
    /// Samples per packet, per channel: what one [`Device::receive_into`] delivers.
    ///
    /// [`Device::receive_into`]: crate::Device::receive_into
    pub samples_per_packet: u32,

    /// The format on the wire, packed by the gateware.
    pub wire: WireFormat,

    /// The format in the host's buffers.
    pub host: HostFormat,

    /// The channels the stream carries.
    pub channels: RxChannels,
}

impl RxConfig {
    /// A channel-0 stream of `samples_per_packet` samples per packet, `host` on the host
    /// and `wire` on the wire: libusdr's `"host@wire"` format.
    #[must_use]
    pub const fn new(samples_per_packet: u32, host: HostFormat, wire: WireFormat) -> Self {
        Self {
            samples_per_packet,
            wire,
            host,
            channels: RxChannels::Channel0,
        }
    }
}

/// A sample format the uSDR's RX front end (`sfe_rx_4`) packs on the wire: complex, one
/// channel's I then Q, little-endian.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WireFormat {
    /// 16 bits per value, 4 bytes per sample (`ci16`, `IFMT_16BIT`).
    #[default]
    Ci16,

    /// 12 bits per value, packed back to back, 3 bytes per sample (`ci12`, `IFMT_12BIT`).
    /// libusdr's 12-bit unpacking reads each value as the top 12 bits of a 16-bit one.
    Ci12,
}

impl WireFormat {
    /// Bits one sample (I and Q) takes on the wire.
    pub(crate) const fn sample_bits(self) -> u32 {
        match self {
            Self::Ci16 => 32,
            Self::Ci12 => 24,
        }
    }

    /// The same format as libusdr's converters name it.
    pub(crate) const fn sample_format(self) -> SampleFormat {
        match self {
            Self::Ci16 => SampleFormat::Ci16,
            Self::Ci12 => SampleFormat::Ci12,
        }
    }
}

/// A host sample format libusdr converts the RX wire formats to (`xdsp/conv.c`); see
/// [`dsp`](crate::dsp) for the arithmetic. Each has its buffer element type, a
/// [`HostSample`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum HostFormat {
    /// [`Complex<i16>`]: full scale ±32767 (`ci16`).
    #[default]
    Ci16,

    /// [`Complex<f32>`]: full scale ±1.0 (`cf32`).
    Cf32,

    /// [`Ci12`]: packed 12-bit pairs, as on the wire (`ci12`).
    Ci12,
}

impl HostFormat {
    /// The same format as libusdr's converters name it.
    pub(crate) const fn sample_format(self) -> SampleFormat {
        match self {
            Self::Ci16 => SampleFormat::Ci16,
            Self::Cf32 => SampleFormat::Cf32,
            Self::Ci12 => SampleFormat::Ci12,
        }
    }
}

/// The channels an RX stream carries. The uSDR has one receiver.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum RxChannels {
    /// Channel 0 (libusdr's `a`): the receiver's I and Q on the front end's lanes 0 and 1.
    #[default]
    Channel0,
}

impl RxChannels {
    /// How many host buffers a receive fills.
    pub(crate) const fn count(self) -> usize {
        match self {
            Self::Channel0 => 1,
        }
    }
}

/// How a stream group starts once started: libusdr's `usdr_dms_sync` types.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SyncMode {
    /// Each stream runs as soon as it starts (`"none"`: the sync unit free-runs).
    #[default]
    None,

    /// The group's streams start together on the first one's activity (`"all"`). With RX
    /// alone, the sync unit waits for the RX front end's activity (`ST_ADCACT`).
    All,

    /// The group starts on an external sync event, such as 1PPS (`"extall"`). libusdr's
    /// `sfetrx4_stream_sync` has no `extall` case and returns `-EINVAL`, so
    /// [`Device::create_streams`](crate::Device::create_streams) refuses it with
    /// [`Error::UnsupportedSync`](crate::Error::UnsupportedSync).
    ExternalAll,
}

/// A time within one stream group, in samples since its streams started: libusdr's
/// host-side `fsymtime`, which counts every packet, lost ones included. It is not the
/// gateware's clock and means nothing across groups.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StreamTime(u64);

impl StreamTime {
    /// A time `samples` samples after the group started.
    #[must_use]
    pub const fn from_samples(samples: u64) -> Self {
        Self(samples)
    }

    /// Samples since the group started.
    #[must_use]
    pub const fn samples(self) -> u64 {
        self.0
    }
}

/// What one [`Device::receive_into`](crate::Device::receive_into) delivered (libusdr's
/// `usdr_dms_recv_nfo`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RxMetadata {
    /// The packet's first sample (`fsymtime`).
    pub time: StreamTime,

    /// Samples written to the front of each channel's buffer: always the stream's packet
    /// size (`totsyms`).
    pub samples: usize,

    /// Packets the gateware reported lost since the group started (`totlost`, which
    /// counts packets despite its header's comment).
    pub lost_packets: u64,
}

/// One complex 12-bit sample, packed as on the wire: I in the low 12 bits of the three
/// little-endian bytes, Q in the high 12. Each value is the top 12 bits of a 16-bit one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Ci12(pub [u8; 3]);

/// A host buffer element: one complex sample in a [`HostFormat`]. Implemented for
/// [`Complex<i16>`], [`Complex<f32>`] and [`Ci12`] only.
pub trait HostSample: Copy + sealed::Sealed {
    /// The format this element holds.
    const FORMAT: HostFormat;
}

impl HostSample for Complex<i16> {
    const FORMAT: HostFormat = HostFormat::Ci16;
}

impl HostSample for Complex<f32> {
    const FORMAT: HostFormat = HostFormat::Cf32;
}

impl HostSample for Ci12 {
    const FORMAT: HostFormat = HostFormat::Ci12;
}

/// Keeps [`HostSample`] to the formats the converters produce, and holds how each element
/// reads from converted bytes.
pub(crate) mod sealed {
    use num_complex::Complex;

    use super::Ci12;

    /// A [`HostSample`](super::HostSample) element as the converters write it.
    pub trait Sealed: Sized {
        /// Bytes one element takes.
        const BYTES: usize;

        /// The element in `bytes`, exactly [`Self::BYTES`] little-endian bytes.
        fn from_le(bytes: &[u8]) -> Self;
    }

    impl Sealed for Complex<i16> {
        const BYTES: usize = 4;

        fn from_le(bytes: &[u8]) -> Self {
            Self::new(
                i16::from_le_bytes([bytes[0], bytes[1]]),
                i16::from_le_bytes([bytes[2], bytes[3]]),
            )
        }
    }

    impl Sealed for Complex<f32> {
        const BYTES: usize = 8;

        fn from_le(bytes: &[u8]) -> Self {
            Self::new(
                f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
                f32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            )
        }
    }

    impl Sealed for Ci12 {
        const BYTES: usize = 3;

        fn from_le(bytes: &[u8]) -> Self {
            Self([bytes[0], bytes[1], bytes[2]])
        }
    }
}
