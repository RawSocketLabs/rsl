//! The stream group and its RX stream: libusdr's `/ll/srx/0` stream on channel 0, created,
//! synchronised, started, read and stopped as `usdr_device_m2_lm6_1_create_stream`,
//! `usdr_dms_sync`, `usdr_dms_op`, `usdr_dms_recv` and `unregister_stream` do it
//! (`device/m2_lm6_1/m2_lm6_1.c`; the stream engine in
//! `ipblks/streams/stream_sfetrx4_dma32.c`). The TX stream is in `tx_stream.rs`.
//!
//! Creating the RX stream powers the receiver, calibrates its DC offsets, configures the
//! FPGA's front end and DMA, holds the sync unit stopped and asks the transport for its
//! buffers; synchronising sets the sync unit's trigger; starting runs the DMA and front
//! end. Blocks flow once both have happened.

use std::time::Duration;

use num_complex::Complex;
use rsl_deps::tracing::{debug, warn};

use super::Board;
use super::tx_stream::TxStream;
use crate::chips::register::run_sequence;
use crate::dsp::Converter;
use crate::error::Error;
use crate::fpga::{BurstPlan, Phy};
use crate::lowlevel::BusError;
use crate::portable::stream::{EngineSequence, StartPart, SyncTrigger};
use crate::stream::{
    HostFormat, HostSample, RxChannels, RxConfig, RxMetadata, StreamTime, StreamsConfig, SyncMode,
    WireFormat,
};

/// The frequency a stream tunes to if none was set, in Hz.
const DEFAULT_RX_HZ: u32 = 320_000_000;

/// Samples converted per step inside the transport's loan: bounds the scratch the
/// converter writes before the samples reach the caller's buffer, or the wire.
pub(super) const CHUNK_SAMPLES: usize = 512;

/// The largest host sample, in bytes (`cf32`).
pub(super) const MAX_HOST_SAMPLE_BYTES: usize = 8;

/// RX burst throttling: of every `send + skip + 2` bursts the front end delivers the first
/// `send + 1` and drops the rest, so a host that cannot keep up sees whole bursts missing
/// rather than overruns (libusdr's `/dm/sdr/0/rfe/throttle`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RxThrottle {
    /// Bursts delivered per cycle, less one.
    pub send: u8,
    /// Bursts dropped per cycle, less one.
    pub skip: u8,
}

/// One packet [`Device::receive`](crate::Device::receive) delivered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RxPacket {
    /// Samples written to the front of the buffer: always the stream's packet size.
    pub samples: usize,

    /// The packet's first sample, counted from the stream's start, lost samples included.
    pub first_sample: u64,

    /// Packets lost since the stream started, as the gateware reports them.
    pub lost_packets: u64,
}

/// The direction of one of a group's streams.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Direction {
    /// The RX stream (`/ll/srx/0`).
    Rx,

    /// The TX stream (`/ll/stx/0`).
    Tx,
}

/// The stream group: the streams created so far and how they start. It exists from the
/// first stream whose transport buffers are set up until the group stops.
#[derive(Debug)]
pub(crate) struct StreamGroup {
    /// The RX stream, if created.
    pub(super) rx: Option<RxStream>,

    /// The TX stream, if created.
    pub(super) tx: Option<TxStream>,

    /// How the group starts. Its trigger follows the streams actually created, as
    /// libusdr's sync follows the handles passed to it.
    mode: SyncMode,

    /// The group has started.
    started: bool,
}

impl StreamGroup {
    /// A group with no stream yet, to start with `mode`.
    const fn new(mode: SyncMode) -> Self {
        Self {
            rx: None,
            tx: None,
            mode,
            started: false,
        }
    }
}

/// A created RX stream's bookkeeping (libusdr's `stream_sfetrx_dma32_t`).
#[derive(Debug)]
pub(crate) struct RxStream {
    /// Samples per packet.
    samples_per_packet: u32,

    /// Where the samples sit in a DMA block.
    layout: BlockLayout,

    /// Bytes per DMA block: one packet's bursts.
    block_bytes: usize,

    /// The host buffers' format.
    host: HostFormat,

    /// The channels, one host buffer each.
    channels: RxChannels,

    /// Wire to host conversion (libusdr's `tf_data`).
    converter: Converter,

    /// Packets received (`rcnt`); the DMA engine is told the host is ready until the first.
    packets: u64,

    /// The next packet's first sample (`r_ts`).
    next_sample: u64,

    /// Packets lost so far (`fe_drop`).
    lost_packets: u64,
}

/// Where a DMA block's samples sit: each burst holds `samples_per_burst` wire samples back
/// to back, then any stub padding it to `bytes_per_burst`.
#[derive(Clone, Copy, Debug)]
struct BlockLayout {
    /// Samples per burst; a packet is a whole number of bursts, and a burst a whole number
    /// of bytes of samples ([`RxConfig::checked_converter`]).
    samples_per_burst: usize,

    /// Bytes per burst, any stub included.
    bytes_per_burst: usize,

    /// Wire bits per sample: 32 for `ci16`, 24 for `ci12`, 16 for `i16`, 12 for `i12`.
    sample_bits: usize,
}

impl SyncMode {
    /// The sync unit's trigger for a group with an RX stream if `rx` and a TX stream if
    /// `tx` (`sfetrx4_stream_sync`).
    ///
    /// # Errors
    ///
    /// [`Error::UnsupportedSync`] for a mode `sfetrx4_stream_sync` has no case for.
    pub(crate) fn trigger(self, rx: bool, tx: bool) -> Result<SyncTrigger, Error> {
        match self {
            Self::None => Ok(SyncTrigger::FreeRun),
            Self::All => Ok(match (rx, tx) {
                (true, true) => SyncTrigger::Duplex,
                (true, false) => SyncTrigger::RxActivity,
                (false, true) => SyncTrigger::TxActivity,
                (false, false) => SyncTrigger::FreeRun,
            }),
            Self::Any => Ok(SyncTrigger::EitherActivity),
            Self::RxActivity => Ok(SyncTrigger::RxActivity),
            Self::TxActivity => Ok(SyncTrigger::TxActivity),
            Self::OnePps => Ok(SyncTrigger::OnePps),
            Self::GeneratedSysref => Ok(SyncTrigger::GeneratedSysref),
            Self::ExternalAll => Err(Error::UnsupportedSync(self)),
        }
    }
}

impl StreamsConfig {
    /// The checks every executor makes before any bus access: a configuration with no
    /// stream, a sync mode with no trigger, a TX stream the ported core cannot take (the
    /// extended core if `extended_tx_core`) and an RX stream the converters or front end
    /// cannot take. Returns the RX stream's wire-to-host converter, if it has one.
    ///
    /// # Errors
    ///
    /// [`Error::NoStreams`], [`Error::UnsupportedSync`], and those of
    /// [`TxConfig::check`] and [`RxConfig::checked_converter`], in that order.
    pub(crate) fn check(self, extended_tx_core: bool) -> Result<Option<Converter>, Error> {
        if self.rx.is_none() && self.tx.is_none() {
            return Err(Error::NoStreams);
        }
        // Refuses a mode with no trigger now; the trigger itself is chosen at start, for
        // the streams the group then holds.
        self.sync.trigger(self.rx.is_some(), self.tx.is_some())?;
        if let Some(tx) = self.tx {
            tx.check(extended_tx_core)?;
        }
        self.rx.map(RxConfig::checked_converter).transpose()
    }
}

impl Board {
    /// Creates the group's streams (libusdr's `usdr_dms_create` for each), RX first,
    /// leaving them stopped until [`Board::start_streams`]. An existing group, a
    /// configuration with no stream, an unsupported sync mode and a stream the gateware
    /// or converters cannot take are refused before any bus access; packet sizes are
    /// checked where libusdr checks them.
    ///
    /// As in libusdr, a stream whose transport buffers are set up stays when a later step
    /// fails, and so does the group holding it, until [`Board::stop_streams`].
    pub(crate) fn create_streams(&mut self, config: StreamsConfig) -> Result<(), Error> {
        self.check_streams(config)?;
        self.create_stream(config, Direction::Rx)?;
        self.create_stream(config, Direction::Tx)
    }

    /// The refusals [`Board::create_streams`] makes before any bus access: an existing
    /// group, then [`StreamsConfig::check`] for this board's gateware.
    pub(crate) fn check_streams(&self, config: StreamsConfig) -> Result<(), Error> {
        if self.streams.is_some() {
            return Err(Error::AlreadyStreaming);
        }
        config.check(self.chains.extended_tx_core).map(|_| ())
    }

    /// Creates `config`'s stream in `direction`, if it has one, joining the group: one
    /// `usdr_dms_create` of [`Board::create_streams`]. Call [`Board::check_streams`]
    /// first; a multi-device aggregate creates each direction on every device in turn.
    pub(crate) fn create_stream(
        &mut self,
        config: StreamsConfig,
        direction: Direction,
    ) -> Result<(), Error> {
        match direction {
            Direction::Rx => {
                let converter = config.check(self.chains.extended_tx_core)?;
                match config.rx.zip(converter) {
                    Some((rx, converter)) => self.create_rx_stream(rx, converter, config.sync),
                    None => Ok(()),
                }
            }
            Direction::Tx => match config.tx {
                Some(tx) => self.create_tx_stream(tx, config.sync),
                None => Ok(()),
            },
        }
    }

    /// Synchronises and starts the group: its [`Board::start_parts`] in one sequence
    /// ([`EngineSequence::start`]: RX alone syncs, then runs; with TX, RX runs, then TX
    /// starts, then the group syncs).
    ///
    /// The trigger is for the streams the group holds: after a failed create left only
    /// RX, `SyncMode::All` waits for RX alone, as libusdr's sync of the handles it has.
    pub(crate) fn start_streams(&mut self) -> Result<(), Error> {
        let (rx, tx_mute, trigger) = self.start_plan()?;
        let mut start = EngineSequence::start(rx, tx_mute, trigger);
        run_sequence(self.bus.as_mut(), &mut start)?;
        self.mark_started(true);
        Ok(())
    }

    /// The parts that start the group, in order ([`StartPart::order`]): those
    /// [`Board::start_streams`] runs as one sequence.
    ///
    /// # Errors
    ///
    /// As [`Board::start_plan`].
    pub(crate) fn start_parts(&self) -> Result<[Option<StartPart>; 3], Error> {
        let (rx, tx_mute, trigger) = self.start_plan()?;
        Ok(StartPart::order(rx, tx_mute, trigger))
    }

    /// What the group's start needs: whether it has an RX stream, its TX stream's mute,
    /// if it has one, and the trigger for those streams in its sync mode.
    ///
    /// # Errors
    ///
    /// [`Error::NotStreaming`] with no group, [`Error::AlreadyStreaming`] once started.
    fn start_plan(&self) -> Result<(bool, Option<u8>, SyncTrigger), Error> {
        let Some(group) = self.streams.as_ref() else {
            return Err(Error::NotStreaming);
        };
        if group.started {
            return Err(Error::AlreadyStreaming);
        }
        let trigger = group.mode.trigger(group.rx.is_some(), group.tx.is_some())?;
        let tx_mute = group.tx.as_ref().map(|tx| tx.mute().bits());
        Ok((group.rx.is_some(), tx_mute, trigger))
    }

    /// Runs one of [`Board::start_parts`]; the group counts as started once `last` has
    /// run. A multi-device aggregate runs each part on every device in turn.
    pub(crate) fn start_part(&mut self, part: StartPart, last: bool) -> Result<(), Error> {
        match self.streams.as_ref() {
            None => return Err(Error::NotStreaming),
            Some(group) if group.started => return Err(Error::AlreadyStreaming),
            Some(_) => {}
        }
        let mut sequence = EngineSequence::new().start_part(part);
        run_sequence(self.bus.as_mut(), &mut sequence)?;
        self.mark_started(last);
        Ok(())
    }

    /// Has the sync unit generate SYSREF (`usdr_dms_sync(.., "sysref+gen", ..)` with no
    /// streams): libusdr's multi-device aggregate does so on its master once every
    /// device waits for 1PPS.
    pub(crate) fn generate_sysref(&mut self) -> Result<(), Error> {
        run_sequence(
            self.bus.as_mut(),
            &mut EngineSequence::new().sync(SyncTrigger::GeneratedSysref),
        )
    }

    /// Records whether the group, if any, has started.
    fn mark_started(&mut self, started: bool) {
        if let Some(group) = self.streams.as_mut() {
            group.started = started;
        }
    }

    /// Creates, starts and free-runs a `ci16` channel-0 RX stream with `samples_per_packet`
    /// samples per packet, in the FFI `usdr` crate's order: create, `USDR_DMS_START`, then
    /// `usdr_dms_sync(.., "none", ..)`.
    pub(crate) fn start_rx_stream(&mut self, samples_per_packet: u32) -> Result<(), Error> {
        let config = RxConfig::new(samples_per_packet, HostFormat::Ci16, WireFormat::Ci16);
        self.create_streams(StreamsConfig::rx(config))?;
        let mut start = EngineSequence::new()
            .run_rx(true)
            .sync(SyncTrigger::FreeRun);
        run_sequence(self.bus.as_mut(), &mut start)?;
        self.streams
            .as_mut()
            .expect("invariant: create_streams just created the group")
            .started = true;
        Ok(())
    }

    /// The group, created to start with `mode` if it does not exist yet: a stream joins it
    /// once its transport buffers are set up.
    pub(super) fn group(&mut self, mode: SyncMode) -> &mut StreamGroup {
        self.streams.get_or_insert(StreamGroup::new(mode))
    }

    /// libusdr's stream create for RX.
    ///
    /// Intentional divergence: libusdr's create then reapplies each channel's LO as a
    /// baseband offset (`usdr_rfic_fe_set_freq(FE_FREQ_BB_RX, .., rx_raw.lo[i])`), which
    /// selects the mixer band (unless the path is fixed), clamps the NCOs and bypasses the
    /// filter until the caller tunes again. That loop is left out, so a new stream keeps
    /// the band, NCOs and filter the caller set.
    fn create_rx_stream(
        &mut self,
        config: RxConfig,
        converter: Converter,
        mode: SyncMode,
    ) -> Result<(), Error> {
        let RxConfig {
            samples_per_packet,
            wire,
            host,
            channels,
        } = config;
        // `usdr_rfic_streaming_up`.
        let waking = !self.rx_running;
        self.rx_running = true;
        self.power_rx()?;
        if waking {
            self.bus.sleep(Duration::from_millis(1));
        }
        if self.rx.lo_hz == 0 {
            self.set_rx_frequency(DEFAULT_RX_HZ)?;
            self.rx.lo_hz = 0;
        }
        self.lms.calibrate_rx_dc(
            self.bus.as_mut(),
            self.reference.hz(),
            (self.rx.vga2a, self.rx.vga2b),
        )?;
        // The calibration tuned the TX PLL to 320 MHz: libusdr tunes it back to the TX LO,
        // when one is set, discarding the result. It tunes the LO itself, not the lowest
        // lock a low-LO fallback found.
        if self.tx.lo_hz != 0 {
            let _ = self.tune_tx_pll(self.tx.lo_hz);
        }

        let bus = self.bus.as_mut();
        run_sequence(bus, &mut EngineSequence::rx_reset())?;
        let plan = BurstPlan::for_packet(samples_per_packet, wire)
            .ok_or(Error::UnsupportedPacketSize(samples_per_packet))?;
        let format = plan.rx_format(channels);
        run_sequence(bus, &mut EngineSequence::rx_configure(&format))?;
        bus.rx_stream_open(plan.block_bytes())
            .map_err(Error::Stream)?;
        let size = |bytes: u32| usize::try_from(bytes).expect("invariant: a block fits in usize");
        self.group(mode).rx = Some(RxStream {
            samples_per_packet,
            layout: BlockLayout {
                samples_per_burst: size(plan.samples_per_burst()),
                bytes_per_burst: size(plan.bytes_per_burst()),
                sample_bits: size(wire.sample_bits()),
            },
            block_bytes: size(plan.block_bytes()),
            host,
            channels,
            converter,
            packets: 0,
            next_sample: 0,
            lost_packets: 0,
        });
        // `usdr_rxupdate_cal`.
        let mut correction = Phy::rx_create_writes(self.rx.iq_imbalance);
        run_sequence(self.bus.as_mut(), &mut correction)?;
        debug!(
            samples_per_packet,
            ?wire,
            ?host,
            samples_per_burst = plan.samples_per_burst(),
            block_bytes = plan.block_bytes(),
            "RX stream created"
        );
        Ok(())
    }

    /// Throttles the stream's bursts, or stops throttling (`_sfetrx4_option_set`'s
    /// "throttle").
    pub(crate) fn set_rx_throttle(&mut self, throttle: Option<RxThrottle>) -> Result<(), Error> {
        if self.rx_packet_samples().is_none() {
            return Err(Error::NotStreaming);
        }
        let throttle = throttle.map(|RxThrottle { send, skip }| (send, skip));
        run_sequence(
            self.bus.as_mut(),
            &mut EngineSequence::new().throttle_rx(throttle),
        )
    }

    /// Samples per packet of the RX stream.
    pub(crate) fn rx_packet_samples(&self) -> Option<u32> {
        let rx = self.streams.as_ref()?.rx.as_ref()?;
        Some(rx.samples_per_packet)
    }

    /// Host buffers one receive fills, with an RX stream.
    pub(crate) fn rx_channel_count(&self) -> Option<usize> {
        let rx = self.streams.as_ref()?.rx.as_ref()?;
        Some(rx.channels.count())
    }

    /// Whether a stream group exists.
    pub(crate) const fn has_streams(&self) -> bool {
        self.streams.is_some()
    }

    /// Receives one `ci16` packet into the front of `samples`: [`Board::receive_into`] for
    /// one channel, reported as an [`RxPacket`].
    pub(crate) fn receive(
        &mut self,
        samples: &mut [Complex<i16>],
        timeout: Duration,
    ) -> Result<RxPacket, Error> {
        let metadata = self.receive_into(&mut [samples], timeout)?;
        Ok(RxPacket {
            samples: metadata.samples,
            first_sample: metadata.time.samples(),
            lost_packets: metadata.lost_packets,
        })
    }

    /// Receives one packet into the front of each channel's buffer, waiting up to
    /// `timeout` (`_sfetrx4_stream_recv`). The block is converted to the host format
    /// burst by burst while the transport lends it, through a bounded scratch.
    ///
    /// Until the first packet arrives, each call first tells the DMA engine the host is
    /// ready, as libusdr does.
    pub(crate) fn receive_into<S: HostSample>(
        &mut self,
        channels: &mut [&mut [S]],
        timeout: Duration,
    ) -> Result<RxMetadata, Error> {
        self.check_receive(channels)?;
        let Some(stream) = self.streams.as_mut().and_then(|group| group.rx.as_mut()) else {
            return Err(Error::NotStreaming);
        };
        let [samples] = channels else {
            unreachable!("invariant: RX streams have one channel until channel maps grow");
        };
        let needed = usize::try_from(stream.samples_per_packet)
            .expect("invariant: a packet size fits in usize");
        let bus = self.bus.as_mut();
        if stream.packets == 0 {
            run_sequence(bus, &mut EngineSequence::new().signal_rx_ready())?;
        }
        let mut oob = [0; 2];
        let mut delivered = 0;
        let (layout, block_bytes, converter) =
            (stream.layout, stream.block_bytes, &stream.converter);
        bus.rx_stream_recv(timeout, &mut |block, words| {
            delivered = block.len();
            if block.len() >= block_bytes {
                layout.convert(converter, block, &mut samples[..needed]);
            }
            oob = words;
        })
        .map_err(|error| match error {
            BusError::Timeout => Error::Timeout,
            other => Error::Stream(other),
        })?;
        if delivered < block_bytes {
            return Err(Error::ShortRxBlock {
                expected: block_bytes,
                got: delivered,
            });
        }

        let lost = oob[0] & 0xff_ffff;
        if lost > 0 {
            warn!(
                lost,
                total = stream.lost_packets + lost,
                "RX packets dropped by the gateware"
            );
        }
        stream.lost_packets += lost;
        stream.next_sample += u64::from(stream.samples_per_packet) * lost;
        let time = StreamTime::from_samples(stream.next_sample);
        stream.next_sample += u64::from(stream.samples_per_packet);
        stream.packets += 1;
        Ok(RxMetadata {
            time,
            samples: needed,
            lost_packets: stream.lost_packets,
            transport_status: oob[1],
        })
    }

    /// The refusals [`Board::receive_into`] makes before any bus access: no RX stream,
    /// buffers not of the stream's host format, not one per channel, or shorter than a
    /// packet.
    pub(crate) fn check_receive<S: HostSample>(&self, channels: &[&mut [S]]) -> Result<(), Error> {
        let Some(stream) = self.streams.as_ref().and_then(|group| group.rx.as_ref()) else {
            return Err(Error::NotStreaming);
        };
        if S::FORMAT != stream.host {
            return Err(Error::HostFormatMismatch {
                stream: stream.host,
                buffer: S::FORMAT,
            });
        }
        let expected = stream.channels.count();
        if channels.len() != expected {
            return Err(Error::ChannelBuffers {
                expected,
                got: channels.len(),
            });
        }
        let needed = usize::try_from(stream.samples_per_packet)
            .expect("invariant: a packet size fits in usize");
        match channels.iter().find(|samples| samples.len() < needed) {
            Some(samples) => Err(Error::BufferTooSmall {
                needed,
                got: samples.len(),
            }),
            None => Ok(()),
        }
    }

    /// Tells the RX DMA engine the host is ready (libusdr's `usdr_dms_set_ready`, the
    /// stream option `"ready"`), as each receive does by itself until the first packet.
    pub(crate) fn set_rx_ready(&mut self) -> Result<(), Error> {
        if self.rx_packet_samples().is_none() {
            return Err(Error::NotStreaming);
        }
        run_sequence(
            self.bus.as_mut(),
            &mut EngineSequence::new().signal_rx_ready(),
        )
    }

    /// Stops and destroys the group's streams: libusdr's `USDR_DMS_STOP` for RX, then TX,
    /// then each stream's destroy, which powers its chain down and releases the
    /// transport's buffers. Every step is attempted; the first failure is returned.
    pub(crate) fn stop_streams(&mut self) -> Result<(), Error> {
        if self.streams.is_none() {
            return Err(Error::NotStreaming);
        }
        let rx_stopped = self.halt_stream(Direction::Rx);
        let tx_stopped = self.halt_stream(Direction::Tx);
        let rx_destroyed = self.destroy_stream(Direction::Rx);
        let tx_destroyed = self.destroy_stream(Direction::Tx);
        rx_stopped
            .and(tx_stopped)
            .and(rx_destroyed)
            .and(tx_destroyed)
    }

    /// Stops the group's stream in `direction`, if it has one (`USDR_DMS_STOP`): one step
    /// of [`Board::stop_streams`], which a multi-device aggregate runs on every device in
    /// turn.
    pub(crate) fn halt_stream(&mut self, direction: Direction) -> Result<(), Error> {
        let Some(group) = self.streams.as_ref() else {
            return Ok(());
        };
        let mut halt = match direction {
            Direction::Rx if group.rx.is_some() => EngineSequence::new().run_rx(false),
            Direction::Tx if group.tx.is_some() => EngineSequence::new().stop_tx(),
            Direction::Rx | Direction::Tx => return Ok(()),
        };
        run_sequence(self.bus.as_mut(), &mut halt)
    }

    /// Destroys the group's stream in `direction`, if it has one (`unregister_stream`),
    /// and the group with its last stream: one step of [`Board::stop_streams`], which a
    /// multi-device aggregate runs on every device in turn.
    pub(crate) fn destroy_stream(&mut self, direction: Direction) -> Result<(), Error> {
        let Some(group) = self.streams.as_mut() else {
            return Ok(());
        };
        let had = match direction {
            Direction::Rx => group.rx.take().is_some(),
            Direction::Tx => group.tx.take().is_some(),
        };
        if group.rx.is_none() && group.tx.is_none() {
            self.streams = None;
        }
        match direction {
            Direction::Rx if had => self.destroy_rx_stream(),
            Direction::Tx if had => self.destroy_tx_stream(),
            Direction::Rx | Direction::Tx => Ok(()),
        }
    }

    /// Destroys the RX stream (`unregister_stream`): powers the receiver down, then
    /// `_sfetrx4_destroy`'s graceful stop and the transport's release. Every step is
    /// attempted; the first failure is returned.
    fn destroy_rx_stream(&mut self) -> Result<(), Error> {
        // `usdr_rfic_streaming_down`.
        self.rx_running = false;
        let powered_down = self.set_rx_power(false);
        let halted = run_sequence(self.bus.as_mut(), &mut EngineSequence::new().run_rx(false));
        let closed = self.bus.rx_stream_close().map_err(Error::Stream);
        debug!("RX stream stopped");
        powered_down.and(halted).and(closed)
    }
}

impl RxConfig {
    /// The stream's wire-to-host converter, once this configuration is one the converters
    /// and the receiver's front end can take; checked before any bus access.
    ///
    /// Intentional divergences: libusdr copies the bytes of a real wire into a complex host
    /// buffer or the reverse (its converter lookup falls back to a copy), takes complex
    /// channel 1 as lanes 2 and 3, and converts a real 12-bit burst that ends mid-byte
    /// whole, stub included; each is refused here.
    fn checked_converter(self) -> Result<Converter, Error> {
        let Self {
            samples_per_packet,
            wire,
            host,
            channels,
        } = self;
        if channels == RxChannels::Channel1 && wire.is_complex() {
            return Err(Error::UnsupportedRxChannels { channels, wire });
        }
        let converter = Converter::new(wire.sample_format(), host.sample_format(), 1, 1)
            .map_err(|_| Error::NoConversion { host, wire })?;
        let half_byte = BurstPlan::for_packet(samples_per_packet, wire)
            .is_some_and(|plan| plan.samples_per_burst() * wire.sample_bits() % 8 != 0);
        if half_byte {
            return Err(Error::UnsupportedPacketSize(samples_per_packet));
        }
        Ok(converter)
    }
}

impl BlockLayout {
    /// Converts a block's samples into `samples`, burst by burst, skipping each burst's
    /// stub, at most [`CHUNK_SAMPLES`] at a time through a stack scratch.
    ///
    /// Divergence from libusdr, invisible on the bus: it converts the block whole, stubs
    /// included, so a padded packet's samples land in the wrong places.
    fn convert<S: HostSample>(self, converter: &Converter, block: &[u8], samples: &mut [S]) {
        const { assert!(S::BYTES <= MAX_HOST_SAMPLE_BYTES) };
        let mut scratch = [0; CHUNK_SAMPLES * MAX_HOST_SAMPLE_BYTES];
        let wire_bytes = |samples: usize| samples * self.sample_bits / 8;
        let bursts = samples
            .chunks_mut(self.samples_per_burst)
            .zip(block.chunks(self.bytes_per_burst));
        for (burst, bytes) in bursts {
            let wire_chunks = bytes.chunks(wire_bytes(CHUNK_SAMPLES));
            for (chunk, wire) in burst.chunks_mut(CHUNK_SAMPLES).zip(wire_chunks) {
                let wire = &wire[..wire_bytes(chunk.len())];
                let host = &mut scratch[..chunk.len() * S::BYTES];
                converter
                    .convert(&[wire], &mut [&mut *host])
                    .expect("invariant: the converter takes one buffer each side");
                for (sample, raw) in chunk.iter_mut().zip(host.chunks_exact(S::BYTES)) {
                    *sample = S::from_le(raw);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::dsp::SampleFormat;
    use crate::stream::Ci12;
    use crate::stream::sealed::Sealed;

    /// Bursts of `samples_per_burst` wire samples of `sample_bytes`, padded to
    /// `bytes_per_burst`.
    fn layout(
        samples_per_burst: usize,
        bytes_per_burst: usize,
        sample_bytes: usize,
    ) -> BlockLayout {
        BlockLayout {
            samples_per_burst,
            bytes_per_burst,
            sample_bits: sample_bytes * 8,
        }
    }

    /// The converter from `from` to `to`, one channel each side.
    fn converter(from: SampleFormat, to: SampleFormat) -> Converter {
        Converter::new(from, to, 1, 1).expect("libusdr has it")
    }

    #[test]
    fn samples_decode_as_little_endian_i_then_q() {
        let block = [0x01, 0x00, 0xff, 0xff, 0x00, 0x80, 0xff, 0x7f];
        let mut samples = [Complex::new(0, 0); 2];
        let ci16 = converter(SampleFormat::Ci16, SampleFormat::Ci16);
        layout(2, 8, 4).convert(&ci16, &block, &mut samples);
        assert_eq!(
            samples,
            [Complex::new(1, -1), Complex::new(i16::MIN, i16::MAX)]
        );
    }

    #[test]
    fn padded_bursts_skip_their_stubs() {
        // Two bursts of one sample, each padded with a 4-byte stub of 0xEE.
        let block = [
            1, 0, 2, 0, 0xee, 0xee, 0xee, 0xee, 3, 0, 4, 0, 0xee, 0xee, 0xee, 0xee,
        ];
        let mut samples = [Complex::new(0, 0); 2];
        let ci16 = converter(SampleFormat::Ci16, SampleFormat::Ci16);
        layout(1, 8, 4).convert(&ci16, &block, &mut samples);
        assert_eq!(samples, [Complex::new(1, 2), Complex::new(3, 4)]);
    }

    #[test]
    fn padded_twelve_bit_bursts_skip_their_stubs() {
        // Two bursts of one ci12 sample each, padded to 8 bytes with 0xEE: I 0x001 and
        // Q 0x002, then I 0xfff (-1) and Q 0x7ff.
        let block = [
            0x01, 0x20, 0x00, 0xee, 0xee, 0xee, 0xee, 0xee, 0xff, 0xff, 0x7f, 0xee, 0xee, 0xee,
            0xee, 0xee,
        ];
        let mut packed = [Ci12::default(); 2];
        let copy = converter(SampleFormat::Ci12, SampleFormat::Ci12);
        layout(1, 8, 3).convert(&copy, &block, &mut packed);
        assert_eq!(packed, [Ci12([0x01, 0x20, 0x00]), Ci12([0xff, 0xff, 0x7f])]);

        let mut wide = [Complex::new(0, 0); 2];
        let unpack = converter(SampleFormat::Ci12, SampleFormat::Ci16);
        layout(1, 8, 3).convert(&unpack, &block, &mut wide);
        // Each value shifted to the top of 16 bits.
        assert_eq!(wide, [Complex::new(0x10, 0x20), Complex::new(-16, 0x7ff0)]);
    }

    #[test]
    fn chunk_boundaries_do_not_change_the_samples() {
        // One unpadded burst longer than two chunks, converted whole as the reference.
        let samples = CHUNK_SAMPLES * 2 + 7;
        let block: Vec<u8> = (0..samples * 3)
            .map(|n| u8::try_from(n * 7 % 251).expect("below 251"))
            .collect();
        let to_float = converter(SampleFormat::Ci12, SampleFormat::Cf32);
        let mut whole = vec![0; samples * 8];
        to_float
            .convert(&[&block], &mut [&mut whole])
            .expect("one buffer each side");
        let expected: Vec<Complex<f32>> = whole.chunks_exact(8).map(Complex::from_le).collect();

        let mut actual = vec![Complex::new(0.0, 0.0); samples];
        layout(samples, block.len(), 3).convert(&to_float, &block, &mut actual);
        assert_eq!(
            actual, expected,
            "chunked conversion equals whole conversion"
        );
    }
}
