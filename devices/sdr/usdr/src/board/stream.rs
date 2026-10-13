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
use crate::dsp::Converter;
use crate::error::Error;
use crate::fpga::{self, BurstPlan, Phy, SyncTrigger};
use crate::lowlevel::BusError;
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
    /// Samples per burst; a packet is a whole number of bursts.
    samples_per_burst: usize,

    /// Bytes per burst, any stub included.
    bytes_per_burst: usize,

    /// Wire bytes per sample: 4 for `ci16`, 3 for `ci12`.
    sample_bytes: usize,
}

impl SyncMode {
    /// The sync unit's trigger for a group with an RX stream if `rx` and a TX stream if
    /// `tx` (`sfetrx4_stream_sync`).
    ///
    /// # Errors
    ///
    /// [`Error::UnsupportedSync`] for a mode `sfetrx4_stream_sync` has no case for.
    fn trigger(self, rx: bool, tx: bool) -> Result<SyncTrigger, Error> {
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

impl Board {
    /// Creates the group's streams (libusdr's `usdr_dms_create` for each), RX first,
    /// leaving them stopped until [`Board::start_streams`]. An existing group, a
    /// configuration with no stream, an unsupported sync mode and a TX stream the gateware
    /// cannot take are refused before any bus access; packet sizes are checked where
    /// libusdr checks them.
    ///
    /// As in libusdr, a stream whose transport buffers are set up stays when a later step
    /// fails, and so does the group holding it, until [`Board::stop_streams`].
    pub(crate) fn create_streams(&mut self, config: StreamsConfig) -> Result<(), Error> {
        if self.streams.is_some() {
            return Err(Error::AlreadyStreaming);
        }
        if config.rx.is_none() && config.tx.is_none() {
            return Err(Error::NoStreams);
        }
        let mode = config.sync;
        // Refuses a mode with no trigger now; the trigger itself is chosen at start, for
        // the streams the group then holds.
        mode.trigger(config.rx.is_some(), config.tx.is_some())?;
        if let Some(tx) = config.tx {
            self.check_tx_stream(tx)?;
        }
        if let Some(rx) = config.rx {
            let converter = Converter::new(rx.wire.sample_format(), rx.host.sample_format(), 1, 1)
                .expect("invariant: libusdr converts every RX wire format to every host format");
            self.create_rx_stream(rx, converter, mode)?;
        }
        if let Some(tx) = config.tx {
            self.create_tx_stream(tx, mode)?;
        }
        Ok(())
    }

    /// Synchronises and starts the group.
    ///
    /// RX alone: `usdr_dms_sync` with the configured mode, then `USDR_DMS_START`, the
    /// order `dm_stream.h` prescribes. With TX: `USDR_DMS_START` for RX, then TX, then the
    /// sync, the order every libusdr client uses (`usdr_dm_create`, the JSON controller,
    /// Soapy), because the TX core's start writes the sync unit's register.
    ///
    /// The trigger is for the streams the group holds: after a failed create left only
    /// RX, `SyncMode::All` waits for RX alone, as libusdr's sync of the handles it has.
    pub(crate) fn start_streams(&mut self) -> Result<(), Error> {
        let Some(group) = self.streams.as_mut() else {
            return Err(Error::NotStreaming);
        };
        if group.started {
            return Err(Error::AlreadyStreaming);
        }
        let trigger = group.mode.trigger(group.rx.is_some(), group.tx.is_some())?;
        let bus = self.bus.as_mut();
        match &group.tx {
            None => {
                fpga::sync(bus, trigger)?;
                if group.rx.is_some() {
                    fpga::run_rx(bus, true)?;
                }
            }
            Some(tx) => {
                if group.rx.is_some() {
                    fpga::run_rx(bus, true)?;
                }
                tx.start(bus)?;
                fpga::sync(bus, trigger)?;
            }
        }
        group.started = true;
        Ok(())
    }

    /// Creates, starts and free-runs a `ci16` channel-0 RX stream with `samples_per_packet`
    /// samples per packet, in the FFI `usdr` crate's order: create, `USDR_DMS_START`, then
    /// `usdr_dms_sync(.., "none", ..)`.
    pub(crate) fn start_rx_stream(&mut self, samples_per_packet: u32) -> Result<(), Error> {
        let config = RxConfig::new(samples_per_packet, HostFormat::Ci16, WireFormat::Ci16);
        self.create_streams(StreamsConfig::rx(config))?;
        let bus = self.bus.as_mut();
        fpga::run_rx(bus, true)?;
        fpga::sync(bus, SyncTrigger::FreeRun)?;
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
        fpga::reset_rx_dma(bus)?;
        let plan = BurstPlan::for_packet(samples_per_packet, wire)
            .ok_or(Error::UnsupportedPacketSize(samples_per_packet))?;
        fpga::configure_rx(bus, &plan)?;
        fpga::sync(bus, SyncTrigger::Stop)?;
        bus.rx_stream_open(plan.block_bytes())
            .map_err(Error::Stream)?;
        let size = |bytes: u32| usize::try_from(bytes).expect("invariant: a block fits in usize");
        self.group(mode).rx = Some(RxStream {
            samples_per_packet,
            layout: BlockLayout {
                samples_per_burst: size(plan.samples_per_burst()),
                bytes_per_burst: size(plan.bytes_per_burst()),
                sample_bytes: size(wire.sample_bits() / 8),
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
        Phy::Rx.set_iq_correction(self.bus.as_mut(), self.rx.iq_imbalance.rx_terms())?;
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
        fpga::throttle_rx(self.bus.as_mut(), throttle)
    }

    /// Samples per packet of the RX stream.
    pub(crate) fn rx_packet_samples(&self) -> Option<u32> {
        let rx = self.streams.as_ref()?.rx.as_ref()?;
        Some(rx.samples_per_packet)
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
        let Some(stream) = self.streams.as_mut().and_then(|group| group.rx.as_mut()) else {
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
        let [samples] = channels else {
            unreachable!("invariant: RX streams have one channel until channel maps grow");
        };
        let needed = usize::try_from(stream.samples_per_packet)
            .expect("invariant: a packet size fits in usize");
        if samples.len() < needed {
            return Err(Error::BufferTooSmall {
                needed,
                got: samples.len(),
            });
        }
        let bus = self.bus.as_mut();
        if stream.packets == 0 {
            fpga::signal_rx_ready(bus)?;
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

    /// Tells the RX DMA engine the host is ready (libusdr's `usdr_dms_set_ready`, the
    /// stream option `"ready"`), as each receive does by itself until the first packet.
    pub(crate) fn set_rx_ready(&mut self) -> Result<(), Error> {
        if self.rx_packet_samples().is_none() {
            return Err(Error::NotStreaming);
        }
        fpga::signal_rx_ready(self.bus.as_mut())
    }

    /// Stops and destroys the group's streams: libusdr's `USDR_DMS_STOP` for RX, then TX,
    /// then each stream's destroy, which powers its chain down and releases the
    /// transport's buffers. Every step is attempted; the first failure is returned.
    pub(crate) fn stop_streams(&mut self) -> Result<(), Error> {
        let Some(group) = self.streams.take() else {
            return Err(Error::NotStreaming);
        };
        let rx_stopped = match group.rx {
            Some(_) => fpga::run_rx(self.bus.as_mut(), false),
            None => Ok(()),
        };
        let tx_stopped = match group.tx {
            Some(_) => fpga::stop_tx(self.bus.as_mut()),
            None => Ok(()),
        };
        let rx_destroyed = match group.rx {
            Some(_) => self.destroy_rx_stream(),
            None => Ok(()),
        };
        let tx_destroyed = match group.tx {
            Some(_) => self.destroy_tx_stream(),
            None => Ok(()),
        };
        rx_stopped
            .and(tx_stopped)
            .and(rx_destroyed)
            .and(tx_destroyed)
    }

    /// Destroys the RX stream (`unregister_stream`): powers the receiver down, then
    /// `_sfetrx4_destroy`'s graceful stop and the transport's release. Every step is
    /// attempted; the first failure is returned.
    fn destroy_rx_stream(&mut self) -> Result<(), Error> {
        // `usdr_rfic_streaming_down`.
        self.rx_running = false;
        let powered_down = self.set_rx_power(false);
        let halted = fpga::run_rx(self.bus.as_mut(), false);
        let closed = self.bus.rx_stream_close().map_err(Error::Stream);
        debug!("RX stream stopped");
        powered_down.and(halted).and(closed)
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
        let bursts = samples
            .chunks_mut(self.samples_per_burst)
            .zip(block.chunks(self.bytes_per_burst));
        for (burst, bytes) in bursts {
            let wire_chunks = bytes.chunks(CHUNK_SAMPLES * self.sample_bytes);
            for (chunk, wire) in burst.chunks_mut(CHUNK_SAMPLES).zip(wire_chunks) {
                let wire = &wire[..chunk.len() * self.sample_bytes];
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
            sample_bytes,
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
