//! The TX stream: libusdr's `/ll/stx/0` in `ci16` on channel A, B or both, created,
//! started, fed and stopped as `usdr_device_m2_lm7_1_create_stream`, `usdr_dms_op`,
//! `usdr_dms_sync`, `usdr_dms_send_stat` and `unregister_stream` do it
//! (`device/m2_lm7_1/m2_lm7_1.c`, `xsdr_ctrl.c`'s `xsdr_hwchans_cnt`, `xsdr_prepare` and
//! `xsdr_reset_extfe`; the original and extended TX cores and the stream engine,
//! `ipblks/streams/sfe_tx_4.c`, `sfe_rx_4.c`'s `exfe_*` and `stream_sfetrx4_dma32.c`, are
//! `sdr-stream`'s, shared with the uSDR).
//!
//! Creating it streams the RFIC's transmitter up (and the receiver with an RX stream),
//! recalibrates the `LimeLight` link with the TX MMCM and the TX phase, checks the
//! gateware is new enough, holds the sync unit stopped and asks the transport for its
//! buffers; on gateware with the extended TX front end (HWID PHY bit 0x08) it first
//! pulses that front end's reset and, after the gateware check, sets it up for the
//! stream's lanes. Starting it starts the TX core on the stream's lanes and lets the sync
//! unit free-run. Each send interleaves the channels' samples into a transport buffer and
//! announces it to the core with its time; a send longer than a packet goes as several,
//! on the original core, as libusdr splits it.

use std::time::Duration;

use lms7002m::Mac;
use lms7002m::ctrl::{Direction, StreamFlags};
use sdr_stream::{
    EngineSequence, ExtendedTxPush, MAX_EXTENDED_TX_TIMESTAMP, MAX_TX_TIMESTAMP, REG_TX_PUSH,
    REG_TX_STATUS, SyncTrigger, TxCoreStatus, TxLanes, extend_drop_count, tx_block_bytes,
    tx_gateware_supported, tx_push_words,
};

use super::{Device, run};
use crate::error::{Access, Error};
use crate::lowlevel::{BusError, TxCommit};
use crate::portable::close::{Down, streaming_down};
use crate::portable::fpga::{Gpo, NAME as FPGA, REG_HWID};
use crate::portable::frontend::XsdrFrontend;
use crate::portable::mmcm::{MmcmLink, MmcmPlan};
use crate::portable::step::Step;

/// Bytes of one `ci16` sample on one channel: I then Q, each a little-endian `i16`.
const SAMPLE_BYTES: usize = 4;

/// General-purpose input word 0, the gateware's build stamp (`IGPI_USR_ACCESS2`), which
/// `/dm/revision` reads before the hardware ID.
const REG_GATEWARE_BUILD: u32 = 16;

/// The transmit channels a stream carries (libusdr's channel mask: 1 for `a`, 2 for `b`, 3
/// for both, mapped by `s_xsdr_chmap`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxChannels {
    /// Channel A: the TX core's lane A, lane B muted.
    A,

    /// Channel B: the lanes swapped, lane A muted.
    B,

    /// Both, A then B in each sample.
    Both,
}

impl TxChannels {
    /// Host buffers one send takes.
    pub(super) const fn count(self) -> usize {
        match self {
            Self::A | Self::B => 1,
            Self::Both => 2,
        }
    }

    /// How a new stream uses the original core's lanes (`initialize_stream_tx_32`,
    /// `fe_tx4_swap_ab_get`): one channel mutes the lane it does not use, channel B swaps
    /// them. The mute is libusdr's two-bit mask, bit 0 lane A.
    const fn created_lanes(self) -> (u8, bool) {
        match self {
            Self::A => (0b10, false),
            Self::B => (0b01, true),
            Self::Both => (0, false),
        }
    }

    /// `log2` of the channel count (`lgchcnt`).
    const fn channels_log2(self) -> u8 {
        match self {
            Self::A | Self::B => 0,
            Self::Both => 1,
        }
    }

    /// The channels' numbers on the extended core's lanes (`s_xsdr_chmap`: `a` 0, `b` 1).
    pub(super) const fn lane_map(self) -> &'static [u8] {
        match self {
            Self::A => &[0],
            Self::B => &[1],
            Self::Both => &[0, 1],
        }
    }
}

/// What the TX core reported with a send's buffer (`usdr_dms_send_stat`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TxTelemetry {
    /// The core's sample clock, low 32 bits (`lhwtime`).
    pub hardware_time: u32,

    /// The fourth status word: the `PCIe` driver's nanosecond time, low 32 bits (`ktime`).
    pub kernel_time: u32,

    /// Bursts the front end and the DMA dropped since the stream was created.
    pub underruns: u64,

    /// How full the core's FIFO is, 0 to 255.
    pub fifo_used: u8,
}

/// A created TX stream's bookkeeping (libusdr's `stream_sfetrx_dma32_t` for TX).
#[derive(Debug)]
pub(super) struct TxStream {
    /// Samples per packet: the most one send takes.
    samples_per_packet: usize,

    /// The channels, one host buffer each.
    pub(super) channels: TxChannels,

    /// The original core's lanes muted (`fe_old_tx_mute`, bit 0 lane A); the extended
    /// core's start mutes nothing.
    pub(super) mute: u8,

    /// The original core's lanes swapped (`fe_old_tx_swap`).
    pub(super) swap: bool,

    /// The extended core's announcing registers, `None` on the original core.
    pub(super) extended: Option<ExtendedTxPush>,

    /// Bursts the front end dropped, extended past its 8-bit counter (`fe_drop`).
    front_end_drops: u64,

    /// Bursts the DMA dropped, likewise (`dma_drop`).
    dma_drops: u64,
}

impl TxStream {
    /// How starting the core sets its lanes (`_sfetrx4_op` starting TX): the mute and
    /// swap, and the channel count.
    pub(super) fn lanes(&self) -> TxLanes {
        TxLanes::new(self.mute, self.swap, self.channels.channels_log2())
            .expect("invariant: two-bit mute and channel count")
    }

    /// Takes a buffer's status words into the drop counts and reports them, as
    /// `_sfetrx4_stream_send` fills `usdr_dms_send_stat`.
    fn account(&mut self, words: [u32; 4]) -> TxTelemetry {
        let status = TxCoreStatus::from_words(words);
        extend_drop_count(&mut self.front_end_drops, status.front_end_drops);
        extend_drop_count(&mut self.dma_drops, status.dma_drops);
        let [_, _, hardware_time, kernel_time] = words;
        TxTelemetry {
            hardware_time,
            kernel_time,
            underruns: self.front_end_drops + self.dma_drops,
            fifo_used: status.fifo_used,
        }
    }
}

impl Device {
    /// Creates the TX stream on `channels` with `samples_per_packet` samples per packet,
    /// in `ci16` (libusdr's `usdr_dms_create` of `/ll/stx/0`), leaving it stopped until
    /// [`Self::start_tx_stream`].
    ///
    /// As libusdr: the `LimeLight` maps are rewritten for the stream's channel count; the
    /// RFIC's TX pins (and RX's, with an RX stream) and the LED come on, its transmitter
    /// (and receiver) is streamed up on both channels, the `LimeLight` link recalibrated
    /// with the TX MMCM and the TX phase, the gateware's build checked, the sync unit held
    /// stopped and the transport's buffers set up; then the maps are rewritten again for
    /// the stream's channels, both banks selected, the TX processor's self-test started
    /// and the DC correctors enabled for the running channels.
    ///
    /// On gateware with the extended TX front end (HWID PHY bit 0x08), as libusdr: after
    /// streaming up, its reset pulsed (`xsdr_reset_extfe`), and after the gateware check
    /// the front end unmuted and set up for the stream's lanes (`exfe_tx4_mute`,
    /// `exfe_tx4_config`, `exfe_trx4_update_chmap`).
    ///
    /// Divergences, before any bus access: without a sample rate on gateware with a TX
    /// MMCM, where libusdr sets 1 MS/s and its TX MMCM configuration then reads past its
    /// table, the create is refused (without a TX MMCM the default rate is set, as in
    /// libusdr, and fails its calibration); so are other links that would (TX rates below
    /// about 6.35 MS/s), packets of no samples (libusdr's "the transport's largest") and,
    /// on the extended core, packets past one burst (the core's FIFO), which libusdr
    /// splits into bursts.
    ///
    /// # Errors
    ///
    /// Before any bus access: [`Error::AlreadyStreaming`] with a TX stream;
    /// [`Error::UnsupportedGateware`] as for [`Self::set_sample_rate`];
    /// [`Error::NoSampleRate`] with a TX MMCM; [`Error::TxClockOutOfRange`];
    /// [`Error::UnsupportedPacketSize`] for 0, or on the extended core past one burst. After the map: [`Error::DoublePumpedMimo`]
    /// for both channels on a double-pumped link. Then streaming up's errors,
    /// [`Error::LinkCalibration`], [`Error::TxClockUnlocked`],
    /// [`Error::OutdatedGateware`], [`Error::UnsupportedPacketSize`] for a packet past the
    /// TX core's FIFO (after the sync unit is stopped, as libusdr checks it),
    /// [`Error::Stream`] when the transport cannot set its buffers up, or [`Error::Bus`].
    ///
    /// # Panics
    ///
    /// Never: the channel count is 1 or 2 and a packet's sample count a `u32`.
    pub fn create_tx_stream(
        &mut self,
        channels: TxChannels,
        samples_per_packet: u32,
    ) -> Result<(), Error> {
        if self.tx_stream.is_some() {
            return Err(Error::AlreadyStreaming);
        }
        let gateware = self.gateware()?;
        let [.., phy] = self.hwid.to_be_bytes();
        let extended = phy & PHY_EXTENDED_TX != 0;
        let count = u32::try_from(channels.count()).expect("1 or 2");
        let sample_bytes = 4 * count;
        if extended && tx_block_bytes(samples_per_packet, sample_bytes).is_none() {
            return Err(Error::UnsupportedPacketSize(samples_per_packet));
        }
        // libusdr's 1 MS/s default reads past its MMCM table with a TX MMCM; without one,
        // its calibration fails the default rate, as here.
        if gateware.tx_mmcm && self.control.clocks().cgen == 0 {
            return Err(Error::NoSampleRate);
        }
        if samples_per_packet == 0 {
            return Err(Error::UnsupportedPacketSize(0));
        }
        if gateware.tx_mmcm && !self.link_clock_fits(false) {
            return Err(Error::TxClockOutOfRange);
        }
        // `xsdr_hwchans_cnt(d, false, logicchs)`: a format without `&` reports 0.
        self.set_hw_channels(Direction::Tx, 0)?;
        // libusdr also marks TX as SISO (`siso_sdr_active_tx`), which only ever matters
        // with the double pump, which already selects SISO DDR.
        if self.rate.double_pump && channels == TxChannels::Both {
            return Err(Error::DoublePumpedMimo);
        }
        self.prepare(self.rx_stream.is_some(), true, gateware)?;
        if extended {
            // `xsdr_reset_extfe`.
            for value in [EXTENDED_TX_RESET, 0] {
                run(
                    &mut *self.bus,
                    &mut Step::fpga(Gpo::TxChainReset.write(value)),
                )?;
            }
        }

        // `/dm/revision`: the build stamp, then the hardware ID.
        let mut build = [0];
        self.bus
            .read_regs(REG_GATEWARE_BUILD, &mut build)
            .map_err(|source| fpga_error(Access::Read, REG_GATEWARE_BUILD, source))?;
        let mut hwid = [0];
        self.bus
            .read_regs(REG_HWID, &mut hwid)
            .map_err(|source| fpga_error(Access::Read, REG_HWID, source))?;
        if !tx_gateware_supported(build[0]) {
            return Err(Error::OutdatedGateware(build[0]));
        }
        if extended {
            let mut setup = EngineSequence::extended_tx_configure(channels.lane_map(), false);
            run(&mut *self.bus, &mut setup)?;
        }
        run(
            &mut *self.bus,
            &mut EngineSequence::new().sync(SyncTrigger::Stop),
        )?;
        let block_bytes = tx_block_bytes(samples_per_packet, sample_bytes)
            .ok_or(Error::UnsupportedPacketSize(samples_per_packet))?;
        self.bus
            .tx_stream_open(block_bytes, sample_bytes)
            .map_err(Error::Stream)?;
        let (mute, swap) = if extended {
            (0, false)
        } else {
            channels.created_lanes()
        };
        self.tx_stream = Some(TxStream {
            samples_per_packet: usize::try_from(samples_per_packet)
                .expect("invariant: a 32-bit count fits in usize"),
            channels,
            mute,
            swap,
            extended: extended.then(ExtendedTxPush::new),
            front_end_drops: 0,
            dma_drops: 0,
        });
        // `xsdr_hwchans_cnt(d, false, hwchs)`, then the processor and DC corrector.
        self.set_hw_channels(Direction::Tx, u8::try_from(count).expect("1 or 2"))?;
        let chip = self.control.chip_mut();
        let mut select = chip.mac_set(Mac::Both);
        run(&mut *self.bus, &mut select)?;
        let mut bist = self.control.chip().tsp_bist_start(lms7002m::Tsp::Tx);
        run(&mut *self.bus, &mut bist)?;
        let (rx, tx) = (self.control.rx_running(), self.control.tx_running());
        let mut dc = self.control.chip().dc_corrector_enable(rx, tx);
        run(&mut *self.bus, &mut dc)
    }

    /// Whether the link the last sample rate left takes the MMCM's every configuration
    /// with `rx_master` as the master ([`MmcmPlan::fits`]).
    pub(super) fn link_clock_fits(&self, rx_master: bool) -> bool {
        let Ok(gateware) = self.gateware() else {
            return true;
        };
        let link = MmcmLink {
            clocks: self.control.clocks(),
            lml: self.control.lml(),
            double_pump: self.rate.double_pump,
            separate_clkdiv: gateware.separate_clkdiv,
            overrides: self.rate.overrides,
        };
        MmcmPlan::fits(&link, rx_master)
    }

    /// Starts the TX stream free-running: libusdr's `USDR_DMS_START` (the TX core stopped,
    /// its buffer pointers reset, then started on the stream's lanes), then
    /// `usdr_dms_sync(.., "none", ..)`.
    ///
    /// # Errors
    ///
    /// [`Error::NotStreaming`] without a TX stream, before any bus access; [`Error::Bus`].
    pub fn start_tx_stream(&mut self) -> Result<(), Error> {
        self.start_tx_stream_with(super::StreamSync::None)
    }

    /// Sends one packet: each channel's buffer whole (channel A's first for
    /// [`TxChannels::Both`]), each sample `[I, Q]`, waiting up to `timeout` for a
    /// transport buffer, to play at `time` in samples or, with `None`, as soon as it
    /// reaches the TX core (`_sfetrx4_stream_send` from `ci16`). Returns the core's
    /// telemetry when the transport had status words for the buffer.
    ///
    /// On a timeout libusdr reads the core's four status words for its log; so does this.
    ///
    /// A send longer than a packet goes as several on the original core, as libusdr
    /// splits it: a packet's worth at a time, each timed the first's time plus the samples
    /// before it, each waiting up to `timeout`; the last buffer's telemetry is returned, or
    /// an earlier one's if the last had none; a send "now" sends every packet "now". A
    /// failure ends the send there.
    ///
    /// Divergences, before any bus access: buffers other than one per channel, of unequal
    /// lengths, empty, and on the extended core longer than a packet (libusdr's burst
    /// split writes past its buffer there), and a time past the core's 47 bits (63 on the
    /// extended core) are refused.
    ///
    /// # Errors
    ///
    /// Before any bus access: [`Error::NotStreaming`]; [`Error::ChannelBuffers`];
    /// [`Error::UnequalChannelBuffers`]; [`Error::SendSize`];
    /// [`Error::UnsupportedTimestamp`]. Then [`Error::Timeout`] when no buffer frees in
    /// time, [`Error::Stream`] for another transport failure or a buffer shorter than the
    /// packet, or [`Error::Bus`].
    ///
    /// # Panics
    ///
    /// Never: a send's sample count is at most a packet's, a `u32`.
    pub fn send(
        &mut self,
        buffers: &[&[[i16; 2]]],
        time: Option<u64>,
        timeout: Duration,
    ) -> Result<Option<TxTelemetry>, Error> {
        let Some(stream) = self.tx_stream.as_mut() else {
            return Err(Error::NotStreaming);
        };
        let expected = stream.channels.count();
        if buffers.len() != expected {
            return Err(Error::ChannelBuffers {
                expected,
                got: buffers.len(),
            });
        }
        let samples = buffers.first().map_or(0, |buffer| buffer.len());
        if let Some(other) = buffers.iter().find(|buffer| buffer.len() != samples) {
            return Err(Error::UnequalChannelBuffers {
                first: samples,
                other: other.len(),
            });
        }
        let packet = stream.samples_per_packet;
        if samples == 0 || (stream.extended.is_some() && samples > packet) {
            return Err(Error::SendSize {
                max: packet,
                got: samples,
            });
        }
        let max_time = if stream.extended.is_some() {
            MAX_EXTENDED_TX_TIMESTAMP
        } else {
            MAX_TX_TIMESTAMP
        };
        if let Some(time) = time.filter(|&time| time > max_time) {
            return Err(Error::UnsupportedTimestamp(time));
        }
        // `_sfetrx4_stream_send`'s split, packet by packet: a time below `INT64_MAX`
        // advances by each; "now" (all ones) stays "now".
        let mut telemetry = None;
        let mut at = time;
        for start in (0..samples).step_by(packet) {
            let end = samples.min(start + packet);
            let mut chunks = [&[][..]; 2];
            for (chunk, buffer) in chunks.iter_mut().zip(buffers) {
                *chunk = &buffer[start..end];
            }
            let sent = self.send_packet(&chunks[..expected], at, timeout)?;
            telemetry = sent.or(telemetry);
            let advance = u64::try_from(end - start).expect("invariant: a packet's samples");
            at = at.map(|time| time + advance);
        }
        Ok(telemetry)
    }

    /// Sends one packet of at most a packet's samples, checked by [`Self::send`].
    fn send_packet(
        &mut self,
        buffers: &[&[[i16; 2]]],
        time: Option<u64>,
        timeout: Duration,
    ) -> Result<Option<TxTelemetry>, Error> {
        let stream = self
            .tx_stream
            .as_mut()
            .expect("invariant: send checked the stream");
        let expected = buffers.len();
        let samples = buffers.first().map_or(0, |buffer| buffer.len());
        let wire_bytes = samples * SAMPLE_BYTES * expected;
        let mut status = None;
        let mut written = 0;
        let sent = self
            .bus
            .tx_stream_send(timeout, time, &mut |buffer, words| {
                status = words;
                let Some(wire) = buffer.get_mut(..wire_bytes) else {
                    return 0;
                };
                interleave(buffers, wire);
                written = wire_bytes;
                wire_bytes
            });
        let commit = match sent {
            Ok(commit) => commit,
            Err(BusError::Timeout) => {
                let mut words = [0; 4];
                self.bus
                    .read_regs(REG_TX_STATUS, &mut words)
                    .map_err(|source| fpga_error(Access::Read, REG_TX_STATUS, source))?;
                return Err(Error::Timeout);
            }
            Err(other) => return Err(Error::Stream(other)),
        };
        if written != wire_bytes {
            return Err(Error::Stream(BusError::Unsupported(
                "TX buffer shorter than the stream's packet",
            )));
        }
        let telemetry = status.map(|words| stream.account(words));
        if commit == TxCommit::Registers {
            let count = u32::try_from(samples).expect("invariant: at most one packet");
            match stream.extended.as_mut() {
                Some(push) => {
                    let bytes = u32::try_from(wire_bytes).expect("invariant: one burst");
                    for &(reg, word) in push.announce(bytes, count, time).as_slice() {
                        self.bus
                            .write_regs(reg, &[word])
                            .map_err(|source| fpga_error(Access::Write, reg, source))?;
                    }
                }
                None => self
                    .bus
                    .write_regs(REG_TX_PUSH, &tx_push_words(count, time))
                    .map_err(|source| fpga_error(Access::Write, REG_TX_PUSH, source))?,
            }
        }
        Ok(telemetry)
    }

    /// Stops and destroys the TX stream: libusdr's `USDR_DMS_STOP` (the TX core stopped),
    /// then `usdr_dms_destroy` (the transmitter streamed down, the core stopped again, the
    /// transport's buffers released, the TX LO forgotten).
    ///
    /// Divergence: every step is attempted, so the buffers are always released, and the
    /// first failure is returned; libusdr's destroy skips the release after a failed
    /// write, and `unregister_stream` reports nothing of it.
    ///
    /// # Errors
    ///
    /// [`Error::NotStreaming`] without a TX stream, before any bus access; [`Error::Bus`]
    /// or [`Error::Stream`] for the first step that failed, except the streaming down's,
    /// whose results libusdr ignores.
    pub fn stop_tx_stream(&mut self) -> Result<(), Error> {
        if self.tx_stream.take().is_none() {
            return Err(Error::NotStreaming);
        }
        let stopped = self.halt_tx();
        self.tx_streaming_down();
        let halted = self.halt_tx();
        let closed = self.bus.tx_stream_close().map_err(Error::Stream);
        self.lo_last[1] = 0;
        stopped.and(halted).and(closed)
    }

    /// The TX core stopped (`sfe_tx4_ctl` stopping, and `_sfetrx4_destroy`'s write).
    pub(super) fn halt_tx(&mut self) -> Result<(), Error> {
        run(&mut *self.bus, &mut EngineSequence::new().stop_tx())
    }

    /// `xsdr_rfic_streaming_down` for TX (`lms7002m_streaming_down`): both banks
    /// selected, the TX processor and front ends off, the AFE left on for RX alone. Every
    /// write is attempted and its result ignored, as `unregister_stream` ignores it.
    fn tx_streaming_down(&mut self) {
        self.rate.afe_active = false;
        let running = (self.control.rx_running(), self.control.tx_running());
        self.control.set_running(running.0, [false; 2]);
        let batches = streaming_down(self.control.chip_mut(), Down::Tx, running);
        for mut batch in batches.into_iter().flatten() {
            let _ = run(&mut *self.bus, &mut batch);
        }
    }

    /// Streams the directions asked up for a stream create, the transmitter's with
    /// [`StreamFlags::default`] on both channels: the chip operation `xsdr_prepare` runs.
    pub(super) fn stream_up(&mut self, rx: bool, tx: bool) -> Result<(), Error> {
        let flags = StreamFlags::default();
        let up = match (rx, tx) {
            (true, true) => self.control.streaming_up(
                XsdrFrontend,
                (Mac::Both, flags),
                Some((Mac::Both, flags)),
            ),
            (true, false) => self.control.streaming_up_rx(XsdrFrontend, Mac::Both, flags),
            (false, true) => self.control.streaming_up_tx(XsdrFrontend, Mac::Both, flags),
            (false, false) => unreachable!("invariant: a stream create streams its direction up"),
        };
        self.run_control(up)
    }
}

/// HWID PHY bit: the extended TX front end (`PHY_EXTENDED_TXFE`), its stream core
/// `CORE_EXFETX_DMA32_R0_2`.
const PHY_EXTENDED_TX: u8 = 0x08;

/// `xsdr_reset_extfe`'s reset of the TX chain's extended front end, bit 2 of
/// [`Gpo::TxChainReset`]; then 0 releases it.
const EXTENDED_TX_RESET: u8 = 0x4;

/// A failed gateware register access in the driver's terms.
fn fpga_error(access: Access, reg: u32, source: BusError) -> Error {
    Error::Bus {
        chip: FPGA,
        access,
        reg,
        source,
    }
}

/// Interleaves the channels' samples into `wire`, each sample time's channels in order,
/// each sample I then Q as little-endian `i16`s: libusdr's `ci16` host to `ci16` wire.
fn interleave(buffers: &[&[[i16; 2]]], wire: &mut [u8]) {
    let channels = buffers.len();
    for (time, chunk) in wire.chunks_exact_mut(SAMPLE_BYTES * channels).enumerate() {
        for (buffer, sample) in buffers.iter().zip(chunk.chunks_exact_mut(SAMPLE_BYTES)) {
            let [i, q] = buffer[time];
            sample[..2].copy_from_slice(&i.to_le_bytes());
            sample[2..].copy_from_slice(&q.to_le_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_channels_interleave_sample_by_sample() {
        let (a, b) = ([[1, 2], [5, 6]], [[3, 4], [7, 8]]);
        let mut wire = [0; 16];
        interleave(&[&a, &b], &mut wire);
        assert_eq!(wire, [1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 6, 0, 7, 0, 8, 0]);
    }

    #[test]
    fn one_channel_mutes_the_lane_it_leaves_and_b_swaps() {
        let word = |channels: TxChannels| {
            let (mute, swap) = channels.created_lanes();
            let lanes = TxLanes::new(mute, swap, channels.channels_log2()).expect("two bits");
            let mut start = EngineSequence::start_lanes(false, Some(lanes), SyncTrigger::Stop);
            let mut last = 0;
            while let Some(hal::SequenceAction::Fpga(hal::FpgaAccess::Write { value, .. })) =
                hal::Sequence::next(&start)
            {
                if value >> 31 == 0 {
                    last = value;
                }
                hal::Sequence::complete::<()>(&mut start, Ok(0)).expect("no failure");
            }
            last
        };
        // `TX_CMD_START_16BIT` (3) with `MODE_MUTEB` (bit 8), `MODE_MUTEA` (bit 9) and
        // `MODE_SWAP_AB` (bit 10), or `MODE_FMT` 1 (bit 3) for two channels.
        assert_eq!(word(TxChannels::A), 0x103);
        assert_eq!(word(TxChannels::B), 0x603);
        assert_eq!(word(TxChannels::Both), 0x00b);
    }
}
