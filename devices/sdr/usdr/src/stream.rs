//! The stream group's public configuration and results: what [`Device::create_streams`]
//! takes, what [`Device::receive_into`] reports and what [`Device::send_with_stat`]
//! returns.
//!
//! The uSDR's streams form one device-owned group: the sync unit they share is per device,
//! so they are created, synchronised, started and stopped together. libusdr's names:
//! `usdr_dms_create` (with its `"host@wire"` format string), `usdr_dms_sync`,
//! `usdr_dms_op`, `usdr_dms_recv` and `usdr_dms_send_stat`.
//!
//! [`Device::create_streams`]: crate::Device::create_streams
//! [`Device::receive_into`]: crate::Device::receive_into
//! [`Device::send_with_stat`]: crate::Device::send_with_stat

use num_complex::Complex;

use crate::dsp::SampleFormat;

/// The streams to create and how to start them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct StreamsConfig {
    /// The RX stream, if any. A group needs at least one stream.
    pub rx: Option<RxConfig>,

    /// The TX stream, if any.
    pub tx: Option<TxConfig>,

    /// How the group starts.
    pub sync: SyncMode,
}

impl StreamsConfig {
    /// A group of one RX stream, started unsynchronised ([`SyncMode::None`]).
    #[must_use]
    pub const fn rx(rx: RxConfig) -> Self {
        Self {
            rx: Some(rx),
            tx: None,
            sync: SyncMode::None,
        }
    }

    /// A group of one TX stream, started unsynchronised ([`SyncMode::None`]).
    #[must_use]
    pub const fn tx(tx: TxConfig) -> Self {
        Self {
            rx: None,
            tx: Some(tx),
            sync: SyncMode::None,
        }
    }

    /// This configuration with `tx` as its TX stream.
    #[must_use]
    pub const fn with_tx(mut self, tx: TxConfig) -> Self {
        self.tx = Some(tx);
        self
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

/// A TX stream: its packet size, the format the host sends and the format the gateware
/// takes, converted from it inside the transport's buffer loan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TxConfig {
    /// Samples per packet, per channel: the most one [`Device::send`] takes, and what each
    /// transport buffer holds.
    ///
    /// [`Device::send`]: crate::Device::send
    pub samples_per_packet: u32,

    /// The format on the wire. The uSDR's TX front end takes [`WireFormat::Ci16`] only.
    pub wire: WireFormat,

    /// The format in the host's buffers.
    pub host: HostFormat,

    /// The channels the stream carries.
    pub channels: TxChannels,
}

impl TxConfig {
    /// A channel-0 stream of up to `samples_per_packet` samples per packet, `host` on the
    /// host and `wire` on the wire: libusdr's `"host@wire"` format.
    #[must_use]
    pub const fn new(samples_per_packet: u32, host: HostFormat, wire: WireFormat) -> Self {
        Self {
            samples_per_packet,
            wire,
            host,
            channels: TxChannels::Channel0,
        }
    }
}

/// A sample format the uSDR's front ends pack on the wire: complex, one channel's I then
/// Q, little-endian. The RX front end (`sfe_rx_4`) packs both; the TX front end
/// (`sfe_tx_4`) takes `Ci16` only.
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

/// A host sample format libusdr converts the wire formats to and from (`xdsp/conv.c`); see
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

/// The channels a TX stream carries. The uSDR has one transmitter.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TxChannels {
    /// Channel 0 (libusdr's `a`): the TX front end's lane A, with lane B muted.
    #[default]
    Channel0,
}

impl TxChannels {
    /// How many host buffers a send takes.
    pub(crate) const fn count(self) -> usize {
        match self {
            Self::Channel0 => 1,
        }
    }
}

/// Which of the TX front end's two lanes are muted: libusdr's stream option `"mute"`, a
/// two-bit mask (bit 0 lane A, bit 1 lane B). A channel-0 stream sends on lane A and is
/// created with [`TxMute::B`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TxMute {
    /// Neither lane (0).
    Neither,

    /// Lane A, which carries channel 0 (1).
    A,

    /// Lane B (2): a channel-0 stream's state at creation.
    #[default]
    B,

    /// Both lanes (3): the transmitter sends nothing.
    Both,
}

impl TxMute {
    /// libusdr's two-bit mask: bit 0 lane A, bit 1 lane B.
    pub(crate) const fn bits(self) -> u8 {
        match self {
            Self::Neither => 0,
            Self::A => 1,
            Self::B => 2,
            Self::Both => 3,
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
    /// alone, the sync unit waits for the RX front end's activity (`ST_ADCACT`); with TX
    /// alone, for the TX front end's (`ST_DACACT`); with both, it starts them together
    /// (`ST_DELAYADCDAC`, libusdr's duplex start).
    All,

    /// The group starts on either front end's activity (`"any"`, `ST_ADCORDACACT`).
    Any,

    /// The group starts on the RX front end's activity, whatever streams it holds (`"rx"`,
    /// `ST_ADCACT`). A TX-only group waits for an RX stream this group cannot start.
    RxActivity,

    /// The group starts on the TX front end's activity, whatever streams it holds (`"tx"`,
    /// `ST_DACACT`). An RX-only group waits for a TX stream this group cannot start.
    TxActivity,

    /// The group starts on a pulse at the board's 1PPS input (`"1pps"`, which libusdr
    /// also spells `"sysref"`: `ST_ONEPPS`).
    OnePps,

    /// The group starts on a SYSREF pulse the sync unit generates itself (`"sysref+gen"`:
    /// `ST_ONEPPS | ST_SYSREF_GEN`, mode 9), which libusdr writes for the uSDR as for any
    /// board. Nothing in the pinned source shows the uSDR's sync unit honours the
    /// generator bit; on gateware that ignores it this is [`SyncMode::OnePps`].
    GeneratedSysref,

    /// The group starts on an external sync event, such as 1PPS (`"extall"`). libusdr's
    /// `sfetrx4_stream_sync` has no `extall` case and returns `-EINVAL`, so
    /// [`Device::create_streams`](crate::Device::create_streams) refuses it with
    /// [`Error::UnsupportedSync`](crate::Error::UnsupportedSync).
    ExternalAll,
}

/// A time within one stream group, in samples since its streams started. Received, it is
/// libusdr's host-side `fsymtime`, which counts every packet, lost ones included. Sent, it
/// is the TX core's timestamp for the packet's first sample, 47 bits of it (libusdr's
/// `dm_time_t`), which libusdr's tools count from the group's start as RX's. It means
/// nothing across groups.
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

/// What the TX core reported with the buffer one
/// [`Device::send_with_stat`](crate::Device::send_with_stat) filled (libusdr's
/// `usdr_dms_send_stat`). libusdr's `opkttime` is left out: it always writes 0 there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TxTelemetry {
    /// The low 32 bits of the TX core's sample clock (`lhwtime`: its third status word,
    /// `STATTS`).
    pub hardware_time: u32,

    /// The fourth status word, raw (`ktime`). Over `PCIe` the driver puts the low 32 bits
    /// of the kernel's nanosecond clock there, taken when it handled the TX completion
    /// that freed this buffer.
    pub kernel_time: u32,

    /// Bursts the TX core dropped since the group started, front end and DMA together
    /// (`underruns`): the core's 8-bit counters, extended across wraps as libusdr does.
    pub underruns: u64,

    /// How full the TX FIFO is, 0 empty to 255 full (`fifo_used`); 0 when the core reports
    /// its USB layout.
    pub fifo_used: u8,
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

    /// The transport's second out-of-band word for the packet, raw (`extra`): over `PCIe`
    /// the RX DMA engine's status, whose fields the pinned source only logs (buffer
    /// counters, front-end state, host ready in bit 31); over USB the TX core's statistics
    /// word the RX trailer carries.
    pub transport_status: u64,
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

    /// A [`HostSample`](super::HostSample) element as the converters read and write it.
    pub trait Sealed: Sized {
        /// Bytes one element takes.
        const BYTES: usize;

        /// The element in `bytes`, exactly [`Self::BYTES`] little-endian bytes.
        fn from_le(bytes: &[u8]) -> Self;

        /// Writes the element into `bytes`, exactly [`Self::BYTES`] long, little-endian.
        fn to_le(&self, bytes: &mut [u8]);
    }

    impl Sealed for Complex<i16> {
        const BYTES: usize = 4;

        fn from_le(bytes: &[u8]) -> Self {
            Self::new(
                i16::from_le_bytes([bytes[0], bytes[1]]),
                i16::from_le_bytes([bytes[2], bytes[3]]),
            )
        }

        fn to_le(&self, bytes: &mut [u8]) {
            bytes[..2].copy_from_slice(&self.re.to_le_bytes());
            bytes[2..4].copy_from_slice(&self.im.to_le_bytes());
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

        fn to_le(&self, bytes: &mut [u8]) {
            bytes[..4].copy_from_slice(&self.re.to_le_bytes());
            bytes[4..8].copy_from_slice(&self.im.to_le_bytes());
        }
    }

    impl Sealed for Ci12 {
        const BYTES: usize = 3;

        fn from_le(bytes: &[u8]) -> Self {
            Self([bytes[0], bytes[1], bytes[2]])
        }

        fn to_le(&self, bytes: &mut [u8]) {
            bytes[..3].copy_from_slice(&self.0);
        }
    }
}
