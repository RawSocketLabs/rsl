//! The RX stream: libusdr's `/ll/srx/0` in `ci16` on channel A, B or both, created,
//! started, read and stopped as `usdr_device_m2_lm7_1_create_stream`, `usdr_dms_sync`,
//! `usdr_dms_op`, `usdr_dms_recv` and `unregister_stream` do it (`device/m2_lm7_1/
//! m2_lm7_1.c`, `xsdr_ctrl.c`'s `xsdr_hwchans_cnt` and `xsdr_prepare`; the stream engine,
//! `ipblks/streams/stream_sfetrx4_dma32.c`, is `sdr-stream`'s, shared with the uSDR).
//!
//! Creating it streams the RFIC's receiver up, recalibrates the `LimeLight` link,
//! configures the FPGA's front end and DMA, holds the sync unit stopped and asks the
//! transport for its buffers; starting it lets the sync unit free-run and starts the DMA
//! and front end.

use std::time::Duration;

use hal::run_sequence;
use lms7002m::ctrl::Direction;
use sdr_stream::{BurstPlan, EngineSequence, RxLanes, RxSampleFormat, SyncTrigger};

use super::{Device, rate_error, run};
use crate::board::PMIC;
use crate::error::Error;
use crate::lowlevel::BusError;
use crate::portable::close::{Down, streaming_down};
use crate::portable::fpga::{Gpo, LMS_LDO_ENABLE, LMS_RESET_RELEASE, LMS_RX_ENABLE, LMS_TX_ENABLE};
use crate::portable::rate::{Gateware, SetSampleRate};
use crate::portable::step::Step;

/// Bytes of one `ci16` sample on one channel: I then Q, each a little-endian `i16`.
const SAMPLE_BYTES: usize = 4;

/// The rate `xsdr_prepare` sets when a stream is created before any (`default_rate`), on
/// the stream's direction alone.
const DEFAULT_RATE: u32 = 1_000_000;

/// The receive channels a stream carries (libusdr's channel mask: 1 for `a`, 2 for `b`, 3
/// for both, mapped to the lanes by `s_xsdr_chmap`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RxChannels {
    /// Channel A: the front end's lanes 1 and 0.
    A,

    /// Channel B: lanes 3 and 2.
    B,

    /// Both, A then B in each sample: all four lanes.
    Both,
}

impl RxChannels {
    /// Host buffers one receive fills.
    const fn count(self) -> usize {
        match self {
            Self::A | Self::B => 1,
            Self::Both => 2,
        }
    }

    /// The lanes `sfe_rx4_configure` selects for the channels' complex samples.
    const fn lanes(self) -> RxLanes {
        match self {
            Self::A => RxLanes::Lanes10,
            Self::B => RxLanes::Lanes32,
            Self::Both => RxLanes::Lanes3210,
        }
    }
}

/// One packet [`Device::receive`] delivered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RxPacket {
    /// Samples written to the front of each channel's buffer: always the stream's packet
    /// size.
    pub samples: usize,

    /// The packet's first sample, counted from the stream's start, lost samples included.
    pub first_sample: u64,

    /// Packets lost since the stream started, as the gateware reports them.
    pub lost_packets: u64,
}

/// A created RX stream's bookkeeping (libusdr's `stream_sfetrx_dma32_t`).
#[derive(Debug)]
pub(super) struct RxStream {
    /// Samples per packet.
    samples_per_packet: usize,

    /// The channels, one host buffer each.
    channels: RxChannels,

    /// Samples per burst; a packet is a whole number of bursts.
    samples_per_burst: usize,

    /// Bytes per burst, any stub padding it to whole front-end words included.
    bytes_per_burst: usize,

    /// Bytes per DMA block: one packet's bursts.
    block_bytes: usize,

    /// Packets received (`rcnt`); the DMA engine is told the host is ready until the first.
    packets: u64,

    /// The next packet's first sample (`r_ts`).
    next_sample: u64,

    /// Packets lost so far (`fe_drop`).
    lost_packets: u64,
}

impl Device {
    /// Creates the RX stream on `channels` with `samples_per_packet` samples per packet,
    /// in `ci16` (libusdr's `usdr_dms_create` of `/ll/srx/0`), leaving it stopped until
    /// [`Self::start_rx_stream`].
    ///
    /// As libusdr: the `LimeLight` maps are rewritten for the stream's channel count if it
    /// changed; without a sample rate, 1 MS/s on RX is set first (and the RFIC powered
    /// up); the RFIC's RX pins and the LED come on, its receiver is streamed up on both
    /// channels, the `LimeLight` link is recalibrated, then the FPGA's RX DMA is reset and
    /// its front end and DMA configured for the packet, the sync unit held stopped and the
    /// transport's buffers set up.
    ///
    /// # Errors
    ///
    /// Before any bus access: [`Error::AlreadyStreaming`] with a stream;
    /// [`Error::UnsupportedGateware`] as for [`Self::set_sample_rate`]; with a TX stream,
    /// [`Error::TxClockOutOfRange`] as for [`Self::create_tx_stream`]. After the map:
    /// [`Error::DoublePumpedMimo`] for both channels on a double-pumped link. Then a
    /// sample rate's or streaming up's errors, [`Error::LinkCalibration`],
    /// [`Error::ChipMissing`] for an RX DMA engine that reads dead,
    /// [`Error::UnsupportedPacketSize`] (after the DMA reset, as libusdr checks it),
    /// [`Error::Stream`] when the transport cannot set its buffers up, or [`Error::Bus`].
    pub fn create_rx_stream(
        &mut self,
        channels: RxChannels,
        samples_per_packet: u32,
    ) -> Result<(), Error> {
        if self.rx_stream.is_some() {
            return Err(Error::AlreadyStreaming);
        }
        let gateware = self.gateware()?;
        let tx = self.tx_stream.is_some();
        if tx && gateware.tx_mmcm && !self.tx_clock_fits() {
            return Err(Error::TxClockOutOfRange);
        }
        // `xsdr_hwchans_cnt(d, true, logicchs)`: a stream format without `&` reports 0
        // logical channels.
        self.set_hw_channels(Direction::Rx, 0)?;
        if self.rate.double_pump {
            if channels == RxChannels::Both {
                return Err(Error::DoublePumpedMimo);
            }
            self.rate.siso_rx = true;
        }
        self.prepare(true, tx, gateware)?;

        let bus = self.bus.as_mut();
        run_sequence(bus, &mut EngineSequence::rx_reset())?;
        let sample_bits = 8 * SAMPLE_BYTES * channels.count();
        let plan = BurstPlan::for_stream(samples_per_packet, bits(sample_bits))
            .ok_or(Error::UnsupportedPacketSize(samples_per_packet))?;
        let format = plan.rx_format(channels.lanes(), RxSampleFormat::Bits16);
        run_sequence(bus, &mut EngineSequence::rx_configure(&format))?;
        bus.rx_stream_open(plan.block_bytes())
            .map_err(Error::Stream)?;
        self.rx_stream = Some(RxStream {
            samples_per_packet: size(samples_per_packet),
            channels,
            samples_per_burst: size(plan.samples_per_burst()),
            bytes_per_burst: size(plan.bytes_per_burst()),
            block_bytes: size(plan.block_bytes()),
            packets: 0,
            next_sample: 0,
            lost_packets: 0,
        });
        Ok(())
    }

    /// `xsdr_prepare(d, rx, tx)`: 1 MS/s in each direction asked if no rate was set (a TX
    /// stream's create refuses that first on gateware with a TX MMCM), the RFIC's pins for the directions and the LED on, the run
    /// flags set, the directions streamed up on both channels, then the link calibration.
    pub(super) fn prepare(&mut self, rx: bool, tx: bool, gateware: Gateware) -> Result<(), Error> {
        if self.control.clocks().cgen == 0 {
            let rate = |on: bool| if on { DEFAULT_RATE } else { 0 };
            self.set_sample_rate(rate(rx), rate(tx))?;
        }
        let mut pins = LMS_LDO_ENABLE | LMS_RESET_RELEASE;
        if rx {
            pins |= LMS_RX_ENABLE;
        }
        if tx {
            pins |= LMS_TX_ENABLE;
        }
        run(&mut *self.bus, &mut Step::fpga(Gpo::LmsPower.write(pins)))?;
        run(&mut *self.bus, &mut Step::fpga(Gpo::Led.write(1)))?;
        self.control.set_running([rx; 2], [tx; 2]);
        self.stream_up(rx, tx)?;
        self.rate.afe_active = true;
        let mut link = SetSampleRate::calibrate_link(&self.control, self.rate, gateware, PMIC);
        let ran = run(&mut *self.bus, &mut link);
        self.control = *link.control();
        self.rate = link.state();
        ran?;
        match link.outcome() {
            Some(Err(error)) => Err(rate_error(error)),
            Some(Ok(())) | None => Ok(()),
        }
    }

    /// Starts the RX stream free-running: libusdr's `usdr_dms_sync(.., "none", ..)`, then
    /// `USDR_DMS_START` (the DMA run, then the front end). Blocks flow from then on.
    ///
    /// # Errors
    ///
    /// [`Error::NotStreaming`] without a stream, before any bus access; [`Error::Bus`].
    pub fn start_rx_stream(&mut self) -> Result<(), Error> {
        if self.rx_stream.is_none() {
            return Err(Error::NotStreaming);
        }
        let mut start = EngineSequence::new()
            .sync(SyncTrigger::FreeRun)
            .run_rx(true);
        run(&mut *self.bus, &mut start)
    }

    /// Receives one packet into the front of each channel's buffer (channel A's first for
    /// [`RxChannels::Both`]), each sample I then Q, waiting up to `timeout`
    /// (`_sfetrx4_stream_recv` into `ci16`). Until the first packet arrives, each call
    /// first tells the DMA engine the host is ready, as libusdr does.
    ///
    /// Divergences, invisible on the bus: a burst padded to whole front-end words (an odd
    /// number of one channel's samples) arrives without its stub, where libusdr copies the
    /// block whole, so its buffer holds each burst's stub as a sample and, in a packet of
    /// several bursts, every later sample shifted along; buffers other than one per
    /// channel, shorter than a packet, or a block shorter than a packet are refused, where
    /// libusdr writes whatever it is given. A refused block counts as no packet: the next
    /// receive signals host-ready again (if none arrived yet) and its lost-packet count is
    /// not added.
    ///
    /// # Errors
    ///
    /// Before any bus access: [`Error::NotStreaming`]; [`Error::ChannelBuffers`] for other
    /// than one buffer per channel; [`Error::BufferTooSmall`] for a buffer shorter than a
    /// packet. Then [`Error::Timeout`] when no packet arrives in time, [`Error::Stream`]
    /// for another transport failure, [`Error::ShortRxBlock`] for a block shorter than a
    /// packet, or [`Error::Bus`].
    ///
    /// # Panics
    ///
    /// Never: a packet's sample count is a `u32`.
    pub fn receive(
        &mut self,
        buffers: &mut [&mut [[i16; 2]]],
        timeout: Duration,
    ) -> Result<RxPacket, Error> {
        let Some(stream) = self.rx_stream.as_mut() else {
            return Err(Error::NotStreaming);
        };
        let expected = stream.channels.count();
        if buffers.len() != expected {
            return Err(Error::ChannelBuffers {
                expected,
                got: buffers.len(),
            });
        }
        let needed = stream.samples_per_packet;
        if let Some(short) = buffers.iter().find(|buffer| buffer.len() < needed) {
            return Err(Error::BufferTooSmall {
                needed,
                got: short.len(),
            });
        }
        if stream.packets == 0 {
            run_sequence(
                self.bus.as_mut(),
                &mut EngineSequence::new().signal_rx_ready(),
            )?;
        }
        let mut oob = [0; 2];
        let mut delivered = 0;
        self.bus
            .rx_stream_recv(timeout, &mut |block, words| {
                delivered = block.len();
                if block.len() >= stream.block_bytes {
                    stream.convert(block, buffers);
                }
                oob = words;
            })
            .map_err(|error| match error {
                BusError::Timeout => Error::Timeout,
                other => Error::Stream(other),
            })?;
        if delivered < stream.block_bytes {
            return Err(Error::ShortRxBlock {
                expected: stream.block_bytes,
                got: delivered,
            });
        }
        let samples = u64::try_from(needed).expect("invariant: a packet size fits in u64");
        let lost = oob[0] & 0xff_ffff;
        stream.lost_packets += lost;
        stream.next_sample += samples * lost;
        let first_sample = stream.next_sample;
        stream.next_sample += samples;
        stream.packets += 1;
        Ok(RxPacket {
            samples: needed,
            first_sample,
            lost_packets: stream.lost_packets,
        })
    }

    /// Stops and destroys the RX stream: libusdr's `USDR_DMS_STOP` (the DMA, then the front
    /// end, stopped), then `usdr_dms_destroy` (the receiver streamed down, the DMA and
    /// front end stopped again, the transport's buffers released, the RX LO forgotten).
    ///
    /// Divergence: every step is attempted, so the buffers are always released, and the
    /// first failure is returned; libusdr's destroy returns at its first failed write,
    /// skipping the second stop of the front end and the buffers' release, and
    /// `unregister_stream` reports nothing of it. The stop's own failure libusdr reports
    /// too (`usdr_dms_op`).
    ///
    /// # Errors
    ///
    /// [`Error::NotStreaming`] without a stream, before any bus access; [`Error::Bus`] or
    /// [`Error::Stream`] for the first step that failed, except the streaming down's,
    /// whose results libusdr ignores.
    pub fn stop_rx_stream(&mut self) -> Result<(), Error> {
        if self.rx_stream.take().is_none() {
            return Err(Error::NotStreaming);
        }
        let stopped = self.halt_rx();
        self.rx_streaming_down();
        let halted = self.halt_rx();
        let closed = self.bus.rx_stream_close().map_err(Error::Stream);
        self.lo_last[0] = 0;
        stopped.and(halted).and(closed)
    }

    /// The RX DMA and front end stopped, DMA first (`_sfetrx4_op` stopping, and
    /// `_sfetrx4_destroy`'s graceful stop).
    pub(super) fn halt_rx(&mut self) -> Result<(), Error> {
        run(&mut *self.bus, &mut EngineSequence::new().run_rx(false))
    }

    /// `xsdr_rfic_streaming_down` for RX (`lms7002m_streaming_down`): both banks
    /// selected, the RX processor and front ends off, the AFE left on for TX alone. Every
    /// write is attempted and its result ignored, as `unregister_stream` ignores it.
    fn rx_streaming_down(&mut self) {
        self.rate.afe_active = false;
        let running = (self.control.rx_running(), self.control.tx_running());
        self.control.set_running([false; 2], running.1);
        let batches = streaming_down(self.control.chip_mut(), Down::Rx, running);
        for mut batch in batches.into_iter().flatten() {
            let _ = run(&mut *self.bus, &mut batch);
        }
    }
}

impl RxStream {
    /// Copies a block's samples into the first [`Self::samples_per_packet`] of each
    /// buffer, burst by burst, skipping each burst's stub; with both channels each sample
    /// time holds A's sample, then B's.
    fn convert(&self, block: &[u8], buffers: &mut [&mut [[i16; 2]]]) {
        let channels = self.channels.count();
        let used = self.samples_per_burst * SAMPLE_BYTES * channels;
        let bursts = (0..self.samples_per_packet)
            .step_by(self.samples_per_burst)
            .zip(block.chunks(self.bytes_per_burst));
        for (first, burst) in bursts {
            let times = burst[..used].chunks_exact(SAMPLE_BYTES * channels);
            for (index, time) in (first..).zip(times) {
                for (buffer, sample) in buffers.iter_mut().zip(time.chunks_exact(SAMPLE_BYTES)) {
                    let [i0, i1, q0, q1] = [sample[0], sample[1], sample[2], sample[3]];
                    buffer[index] = [i16::from_le_bytes([i0, i1]), i16::from_le_bytes([q0, q1])];
                }
            }
        }
    }
}

/// A sample's width in bits as the engine's 32-bit arithmetic takes it.
fn bits(bits: usize) -> u32 {
    u32::try_from(bits).expect("invariant: at most two channels' bits")
}

/// A register-sized count as a buffer length.
fn size(count: u32) -> usize {
    usize::try_from(count).expect("invariant: a 32-bit count fits in usize")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-channel stream of `samples` per packet in bursts of `samples_per_burst`, each
    /// padded to `bytes_per_burst`.
    fn stream(samples: usize, samples_per_burst: usize, bytes_per_burst: usize) -> RxStream {
        RxStream {
            samples_per_packet: samples,
            channels: RxChannels::A,
            samples_per_burst,
            bytes_per_burst,
            block_bytes: bytes_per_burst * samples / samples_per_burst,
            packets: 0,
            next_sample: 0,
            lost_packets: 0,
        }
    }

    #[test]
    fn padded_bursts_skip_their_stubs() {
        // Three bursts of one sample, each padded with a 4-byte stub of 0xEE.
        let mut block = Vec::new();
        for n in 1..=3_u8 {
            block.extend([n, 0, 0x10 + n, 0, 0xee, 0xee, 0xee, 0xee]);
        }
        let mut samples = [[0; 2]; 3];
        stream(3, 1, 8).convert(&block, &mut [&mut samples]);
        assert_eq!(
            samples,
            [[1, 0x11], [2, 0x12], [3, 0x13]],
            "each burst's sample, the stubs between them left out"
        );
    }

    #[test]
    fn both_channels_split_sample_by_sample() {
        let block = [1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 6, 0, 7, 0, 8, 0];
        let mut stream = stream(2, 2, 16);
        stream.channels = RxChannels::Both;
        let (mut a, mut b) = ([[0; 2]; 2], [[0; 2]; 2]);
        stream.convert(&block, &mut [&mut a, &mut b]);
        assert_eq!((a, b), ([[1, 2], [5, 6]], [[3, 4], [7, 8]]));
    }
}
