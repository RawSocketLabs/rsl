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
use lms7002m::ctrl::StreamFlags;
use lms7002m::{Mac, RxMode, RxPath, Tsp};
use sdr_stream::{BurstPlan, EngineSequence, RxLanes, RxSampleFormat, SyncTrigger};

use super::{Device, control_error, run};
use crate::board::XSDR_SUPPLIES;
use crate::error::Error;
use crate::lowlevel::BusError;
use crate::portable::fpga::{Gpo, LMS_LDO_ENABLE, LMS_RESET_RELEASE, LMS_RX_ENABLE};
use crate::portable::frontend::XsdrFrontend;
use crate::portable::rate::{Gateware, RateError, SetSampleRate};
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
    /// [`Error::UnsupportedGateware`] as for [`Self::set_sample_rate`]. After the map:
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
        // `xsdr_hwchans_cnt(d, true, logicchs)`: a stream format without `&` reports 0
        // logical channels.
        if self.hw_channels[0] != 0 {
            self.hw_channels[0] = 0;
            let pumped = self.rate.double_pump;
            let tx_siso = self.hw_channels[1] == 1 || pumped;
            let op = self.control.set_siso(XsdrFrontend, pumped, tx_siso);
            self.run_control(op)?;
        }
        if self.rate.double_pump {
            if channels == RxChannels::Both {
                return Err(Error::DoublePumpedMimo);
            }
            self.rate.siso_rx = true;
        }
        self.prepare_rx(gateware)?;

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

    /// `xsdr_prepare` for RX alone: a default rate if none was set, the RFIC's RX pins and
    /// the LED on, its receiver streamed up on both channels, then the link calibration.
    fn prepare_rx(&mut self, gateware: Gateware) -> Result<(), Error> {
        if self.control.clocks().cgen == 0 {
            self.set_sample_rate(DEFAULT_RATE, 0)?;
        }
        let pins = LMS_LDO_ENABLE | LMS_RESET_RELEASE | LMS_RX_ENABLE;
        run(&mut *self.bus, &mut Step::fpga(Gpo::LmsPower.write(pins)))?;
        run(&mut *self.bus, &mut Step::fpga(Gpo::Led.write(1)))?;
        self.control.set_running([true; 2], [false; 2]);
        let up = self
            .control
            .streaming_up_rx(XsdrFrontend, Mac::Both, StreamFlags::default());
        self.run_control(up)?;
        self.rate.afe_active = true;
        let mut link =
            SetSampleRate::calibrate_link(&self.control, self.rate, gateware, XSDR_SUPPLIES.pmic);
        let ran = run(&mut *self.bus, &mut link);
        self.control = *link.control();
        self.rate = link.state();
        ran?;
        match link.outcome() {
            Some(Err(RateError::Control(error))) => Err(control_error(error)),
            Some(Err(RateError::Calibration)) => Err(Error::LinkCalibration),
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
    /// Divergence, invisible on the bus: a burst padded to whole front-end words (an odd
    /// number of one channel's samples) arrives without its stub, which libusdr copies in
    /// with the samples.
    ///
    /// # Errors
    ///
    /// Before any bus access: [`Error::NotStreaming`]; [`Error::ChannelBuffers`] for other
    /// than one buffer per channel; [`Error::BufferTooSmall`] for a buffer shorter than a
    /// packet. Then [`Error::Timeout`] when no packet arrives in time, [`Error::Stream`]
    /// for another transport failure, [`Error::ShortRxBlock`] for a block shorter than a
    /// packet, or [`Error::Bus`].
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
        let samples = u64::try_from(needed).unwrap_or(u64::MAX);
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
    /// Every step is attempted.
    ///
    /// Divergence: the first failure is returned; libusdr's destroy ignores its own.
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
        let tx_running = self.control.tx_running();
        self.control.set_running([false; 2], tx_running);
        let chip = self.control.chip_mut();
        let batches = [
            chip.mac_set(Mac::Both),
            chip.tsp_enable(Tsp::Rx, false),
            chip.rfe_path(RxPath::None, RxMode::Disable)
                .expect("invariant: both banks are selected just before"),
            chip.afe_enable([false; 2], tx_running),
        ];
        for mut batch in batches {
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
