//! TX streaming: libusdr's `/ll/stx/0` stream, created, started, fed, muted, remapped and
//! destroyed as `usdr_device_m2_lm6_1_create_stream`'s TX branch, `usdr_dms_op`,
//! `usdr_dms_send_stat`, the stream options `"mute"` and `"chmap"` and `unregister_stream`
//! do it (`device/m2_lm6_1/m2_lm6_1.c`; the stream engine in
//! `ipblks/streams/stream_sfetrx4_dma32.c`, `sfe_tx_4.c` and, for the extended core,
//! `sfe_rx_4.c`'s `exfe_*`). The group around it is in `stream.rs`.
//!
//! Creating the stream powers the transmitter, checks the gateware is new enough, holds
//! the sync unit stopped, asks the transport for its buffers, turns the constant generator
//! off and writes the IQ correction again. On gateware with the extended TX core (hardware
//! ID bit 26) it first pulses the TX chain's extended front-end reset and, after the
//! gateware check, sets that front end up for the stream's lanes and sample width. Each
//! send converts the host's samples into a transport buffer and announces it to the TX core
//! with its time.

use std::time::Duration;

use deps::tracing::{debug, warn};

use super::Board;
use super::stream::{CHUNK_SAMPLES, MAX_CHANNELS, MAX_HOST_SAMPLE_BYTES};
use crate::chips::register::run_sequence;
use crate::dsp::Converter;
use crate::error::Error;
use crate::fpga::{
    self, ExtendedTxPush, MAX_EXTENDED_TX_TIMESTAMP, MAX_TX_TIMESTAMP, Phy, TxCoreStatus,
};
use crate::lowlevel::{BusError, TxCommit};
use crate::portable::radio::{Chain, Chains, StreamUp, stream_down};
use crate::portable::stream::{EngineSequence, SyncTrigger, TxLanes};
use crate::stream::{
    HostFormat, HostSample, StreamTime, SyncMode, TxChannels, TxConfig, TxMute, TxTelemetry,
    WireFormat,
};

/// A created TX stream's bookkeeping (libusdr's `stream_sfetrx_dma32_t` for TX).
#[derive(Debug)]
pub(crate) struct TxStream {
    /// Samples per packet, per channel: the most one send takes.
    samples_per_packet: u32,

    /// The host buffers' format.
    host: HostFormat,

    /// The channels, one host buffer each, and how they use the core's lanes.
    lanes: TxLaneUse,

    /// Host to wire conversion (libusdr's `tf_data`).
    converter: Converter,

    /// Wire bytes of one sample of every channel.
    sample_bytes: usize,

    /// The extended core's announcing registers, `None` on the original core.
    extended: Option<ExtendedTxPush>,

    /// Bursts the front end dropped so far, extended past its 8-bit counter (`fe_drop`).
    front_end_drops: u64,

    /// Bursts the DMA dropped so far, likewise (`dma_drop`).
    dma_drops: u64,
}

impl TxStream {
    /// How starting the core sets its lanes: the stream's [`TxLaneUse::lanes`].
    pub(super) const fn lanes(&self) -> TxLanes {
        self.lanes.lanes()
    }

    /// Takes a buffer's status words into the drop counts and reports them, as
    /// `_sfetrx4_stream_send` fills `usdr_dms_send_stat`.
    fn account(&mut self, words: [u32; 4]) -> TxTelemetry {
        let status = TxCoreStatus::from_words(words);
        fpga::extend_drop_count(&mut self.front_end_drops, status.front_end_drops);
        fpga::extend_drop_count(&mut self.dma_drops, status.dma_drops);
        let [_, _, hardware_time, kernel_time] = words;
        TxTelemetry {
            hardware_time,
            kernel_time,
            underruns: self.front_end_drops + self.dma_drops,
            fifo_used: status.fifo_used,
        }
    }
}

/// How a TX stream uses the TX core's two lanes: its channels, the lanes muted
/// (`fe_old_tx_mute`) and whether they swap (`fe_old_tx_swap`), which starting the core
/// applies. The blocking stream and `NativeUsb`'s group each keep one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TxLaneUse {
    /// The channels, one host buffer each.
    pub(crate) channels: TxChannels,

    /// The lanes muted. Always [`TxMute::Neither`] on the extended core, whose own mute
    /// libusdr writes to its front end instead.
    pub(crate) mute: TxMute,

    /// The original core's lanes swapped, a channel-1 stream.
    pub(crate) swap: bool,
}

impl TxLaneUse {
    /// How a new stream on `channels` uses the lanes (`initialize_stream_tx_32`): on the
    /// original core, as [`Self::lane_use`] gives them; on the `extended` core nothing is
    /// muted or swapped.
    pub(crate) const fn created(channels: TxChannels, extended: bool) -> Self {
        let (mute, swap) = if extended {
            (TxMute::Neither, false)
        } else {
            Self::lane_use(channels, None)
        };
        Self {
            channels,
            mute,
            swap,
        }
    }

    /// How starting the core sets its lanes (`_sfetrx4_op` starting TX): the mute and
    /// swap, and the channel count (`fe_chans`).
    pub(crate) const fn lanes(self) -> TxLanes {
        let channels_log2 = match self.channels {
            TxChannels::Channel0 | TxChannels::Channel1 => 0,
            TxChannels::Both => 1,
        };
        TxLanes::new(self.mute.bits(), self.swap, channels_log2)
            .expect("invariant: a TxMute is two bits and the channel count at most two")
    }

    /// Moves the stream to `channels`, of the same count (`_sfetrx4_option_set`'s
    /// `"chmap"`), keeping the new use, and returns the engine writes that apply it: on
    /// the original core, the lanes swap as a new stream's would and a one-channel stream
    /// muting a lane mutes the other instead (`fe_tx4_swap_ab_get`, `sfe_tx4_upd`); on the
    /// `extended` core, its lane map is rewritten (`exfe_trx4_update_chmap`). The
    /// blocking stream and `NativeUsb`'s group both run it. The new use is kept before it
    /// is written, so a failed write leaves it ahead of the core's: libusdr's order on the
    /// original core; on the extended core libusdr keeps no channel state, and the count,
    /// all that is read of it there, does not change.
    ///
    /// Intentional divergence: libusdr reads only as many entries of a new map as the
    /// stream has channels, so a map of another count still applies; it is refused here,
    /// as is a second channel without the TX chain (`tx_chain`).
    ///
    /// # Errors
    ///
    /// [`Error::UnsupportedTxChannels`], before any change.
    pub(crate) fn remap(
        &mut self,
        channels: TxChannels,
        extended: bool,
        tx_chain: bool,
    ) -> Result<EngineSequence, Error> {
        let second_channel = matches!(channels, TxChannels::Channel1 | TxChannels::Both);
        if channels.count() != self.channels.count() || (second_channel && !tx_chain) {
            return Err(Error::UnsupportedTxChannels { channels });
        }
        if extended {
            self.channels = channels;
            return Ok(EngineSequence::new().extended_tx_map(channels.lane_map()));
        }
        let (mute, swap) = Self::lane_use(channels, Some(self.mute));
        *self = Self {
            channels,
            mute,
            swap,
        };
        Ok(EngineSequence::new().update_tx_lanes(self.lanes()))
    }

    /// The original core's mute and swap for `channels` at creation, or after a remap with
    /// the stream's `mute` (`initialize_stream_tx_32`, `_sfetrx4_option_set`'s `"chmap"`):
    /// one channel mutes the lane it does not use, unless remapped while muting neither;
    /// two channels mute neither.
    const fn lane_use(channels: TxChannels, mute: Option<TxMute>) -> (TxMute, bool) {
        let swap = matches!(channels, TxChannels::Channel1);
        let mute = match (channels, mute) {
            (TxChannels::Both, None) => TxMute::Neither,
            (TxChannels::Both, Some(mute)) | (_, Some(mute @ TxMute::Neither)) => mute,
            (TxChannels::Channel1, _) => TxMute::A,
            (TxChannels::Channel0, _) => TxMute::B,
        };
        (mute, swap)
    }
}

impl TxConfig {
    /// Refuses, before any bus access, a TX stream the gateware's TX core, given its
    /// `chains`, cannot take: a second channel without the TX chain; a wire format other
    /// than `ci16` on the original core, or than `ci16` and `ci12` on the extended one; a
    /// host format with no converter to the wire for the stream's channels; a packet size
    /// of 0; and on the extended core, a packet over one burst.
    ///
    /// Intentional divergences: libusdr takes a size of 0 to mean the transport's largest
    /// buffer (its `PCIe` driver's, capped at the core's FIFO); a `TxConfig` names its size.
    /// libusdr copies a real host buffer's bytes onto the complex wire unconverted (its
    /// converter lookup falls back to a copy); it refuses the original core's other wires
    /// only after powering the transmitter. It takes a second channel on gateware without
    /// the TX chain, which it reports as one channel. On the extended core it splits a
    /// packet over the core's FIFO into two to eight bursts, which its USB transport does
    /// not frame (it sends the first).
    pub(super) fn check(self, chains: Chains) -> Result<(), Error> {
        let second_channel = matches!(self.channels, TxChannels::Channel1 | TxChannels::Both);
        if second_channel && !chains.tx {
            return Err(Error::UnsupportedTxChannels {
                channels: self.channels,
            });
        }
        let wire_ok = match self.wire {
            WireFormat::Ci16 => true,
            WireFormat::Ci12 => chains.extended_tx_core,
            WireFormat::I16 | WireFormat::I12 => false,
        };
        if !wire_ok {
            return Err(Error::UnsupportedWireFormat(self.wire));
        }
        let inputs = self.channels.count();
        if Converter::new(
            self.host.sample_format(),
            self.wire.sample_format(),
            inputs,
            1,
        )
        .is_err()
        {
            return Err(Error::NoConversion {
                host: self.host,
                wire: self.wire,
            });
        }
        if self.samples_per_packet == 0 {
            return Err(Error::UnsupportedPacketSize(0));
        }
        if chains.extended_tx_core {
            fpga::stream_block_bytes(self.samples_per_packet, self.sample_bytes())?;
        }
        Ok(())
    }

    /// Wire bytes of one sample of every channel, for the complex wires a TX stream takes
    /// (a real 12-bit sample is not a whole byte).
    pub(crate) fn sample_bytes(self) -> u32 {
        let per_channel = self.wire.sample_bits() / 8;
        per_channel * u32::try_from(self.channels.count()).expect("1 or 2")
    }
}

impl Board {
    /// libusdr's stream create for TX (`initialize_stream_tx_32`), on the gateware's TX
    /// core, joining the group made to start with `mode`. Call [`TxConfig::check`] first.
    ///
    /// Divergence on a failed read only: libusdr logs a failed gateware read and compares
    /// whatever it holds; this returns the failure.
    pub(super) fn create_tx_stream(
        &mut self,
        config: TxConfig,
        mode: SyncMode,
    ) -> Result<(), Error> {
        let TxConfig {
            samples_per_packet,
            wire,
            host,
            channels,
        } = config;
        let converter = Converter::new(
            host.sample_format(),
            wire.sample_format(),
            channels.count(),
            1,
        )
        .expect("invariant: TxConfig::check refused hosts with no converter to the wire");
        let extended = self.radio.chains.extended_tx_core;
        // `usdr_rfic_streaming_up`: libusdr takes its wake-up wait even when powering fails.
        self.run_radio(StreamUp::new(self.radio, Chain::Tx))?;
        if extended {
            // `usdr_reset_txfex`.
            run_sequence(self.bus.as_mut(), &mut Phy::tx_extended_reset_writes())?;
        }
        fpga::check_tx_gateware(self.identity()?.gateware_build)?;
        let bus = self.bus.as_mut();
        if extended {
            let twelve_bit = wire == WireFormat::Ci12;
            let mut setup = EngineSequence::extended_tx_configure(channels.lane_map(), twelve_bit);
            run_sequence(bus, &mut setup)?;
        }
        run_sequence(bus, &mut EngineSequence::new().sync(SyncTrigger::Stop))?;
        let sample_bytes = config.sample_bytes();
        let block_bytes = fpga::stream_block_bytes(samples_per_packet, sample_bytes)?;
        bus.tx_stream_open(block_bytes, sample_bytes)
            .map_err(Error::Stream)?;
        self.group(mode).tx = Some(TxStream {
            samples_per_packet,
            host,
            lanes: TxLaneUse::created(channels, extended),
            converter,
            sample_bytes: usize::try_from(sample_bytes).expect("a few bytes"),
            extended: extended.then(ExtendedTxPush::new),
            front_end_drops: 0,
            dma_drops: 0,
        });
        // `usdr_tx_gen_set(.., false, ..)`, then `usdr_txupdate_cal`.
        let mut dsp = Phy::tx_create_writes(self.corrections.tx);
        run_sequence(self.bus.as_mut(), &mut dsp)?;
        debug!(
            samples_per_packet,
            ?host,
            ?wire,
            ?channels,
            block_bytes,
            extended,
            "TX stream created"
        );
        Ok(())
    }

    /// Destroys the TX stream (`unregister_stream`): powers the transmitter down, then
    /// `_sfetrx4_destroy` clears the core's control register and releases the transport.
    /// Every step is attempted; the first failure is returned. libusdr skips the
    /// transport's release when the register write fails; this releases it anyway, as the
    /// RX stream's stop does, since the group is gone either way.
    pub(super) fn destroy_tx_stream(&mut self) -> Result<(), Error> {
        // `usdr_rfic_streaming_down`.
        let mut down = stream_down(self.radio, Chain::Tx);
        let powered_down = run_sequence(self.bus.as_mut(), &mut down);
        self.radio = down.radio();
        let halted = run_sequence(self.bus.as_mut(), &mut EngineSequence::new().stop_tx());
        let closed = self.bus.tx_stream_close().map_err(Error::Stream);
        debug!("TX stream stopped");
        powered_down.and(halted).and(closed)
    }

    /// The refusals [`Board::send`] makes before any bus access: no TX stream, buffers
    /// not of the stream's host format, not one per channel, of unequal lengths, or holding
    /// no samples or more than a packet, and a time past what the TX core takes (47 bits
    /// on the original core, 63 on the extended).
    pub(crate) fn check_send<S: HostSample>(
        &self,
        channels: &[&[S]],
        time: Option<StreamTime>,
    ) -> Result<(), Error> {
        let Some(stream) = self.streams.as_ref().and_then(|group| group.tx.as_ref()) else {
            return Err(Error::NotStreaming);
        };
        if S::FORMAT != stream.host {
            return Err(Error::HostFormatMismatch {
                stream: stream.host,
                buffer: S::FORMAT,
            });
        }
        let expected = stream.lanes.channels.count();
        if channels.len() != expected {
            return Err(Error::ChannelBuffers {
                expected,
                got: channels.len(),
            });
        }
        let samples = channels.first().map_or(0, |channel| channel.len());
        if let Some(other) = channels.iter().find(|channel| channel.len() != samples) {
            return Err(Error::UnequalChannelBuffers {
                first: samples,
                other: other.len(),
            });
        }
        let max = usize::try_from(stream.samples_per_packet)
            .expect("invariant: a packet size fits in usize");
        if let Some(samples) = channels
            .iter()
            .find(|samples| samples.is_empty() || samples.len() > max)
        {
            return Err(Error::SendSize {
                max,
                got: samples.len(),
            });
        }
        let max_time = match stream.extended {
            Some(_) => MAX_EXTENDED_TX_TIMESTAMP,
            None => MAX_TX_TIMESTAMP,
        };
        match time {
            Some(time) if time.samples() > max_time => Err(Error::UnsupportedTimestamp(time)),
            _ => Ok(()),
        }
    }

    /// Host buffers one send takes, with a TX stream.
    pub(crate) fn tx_channel_count(&self) -> Option<usize> {
        let tx = self.streams.as_ref()?.tx.as_ref()?;
        Some(tx.lanes.channels.count())
    }

    /// Mutes the core's lanes as `mute` says (`_sfetrx4_option_set`'s `"mute"`). On the
    /// original core the stream keeps it, and starting the stream applies it too; on the
    /// extended core it goes to the front end's mute register, raw, and the start mutes
    /// nothing, as in libusdr.
    pub(crate) fn set_tx_mute(&mut self, mute: TxMute) -> Result<(), Error> {
        let Some(stream) = self.streams.as_mut().and_then(|group| group.tx.as_mut()) else {
            return Err(Error::NotStreaming);
        };
        if stream.extended.is_some() {
            let mut update = EngineSequence::new().extended_tx_mute(mute.bits().into());
            return run_sequence(self.bus.as_mut(), &mut update);
        }
        stream.lanes.mute = mute;
        let mut update = EngineSequence::new().update_tx_lanes(stream.lanes());
        run_sequence(self.bus.as_mut(), &mut update)
    }

    /// Moves the stream to `channels`, of the same count (`_sfetrx4_option_set`'s
    /// `"chmap"`), as [`TxLaneUse::remap`] says.
    pub(crate) fn set_tx_channels(&mut self, channels: TxChannels) -> Result<(), Error> {
        let tx_chain = self.radio.chains.tx;
        let Some(stream) = self.streams.as_mut().and_then(|group| group.tx.as_mut()) else {
            return Err(Error::NotStreaming);
        };
        let mut update = stream
            .lanes
            .remap(channels, stream.extended.is_some(), tx_chain)?;
        run_sequence(self.bus.as_mut(), &mut update)
    }

    /// Sends one packet: the front of each channel's buffer, all of it, waiting up to
    /// `timeout` for a transport buffer, to play at `time` or, with `None`, as soon as it
    /// reaches the core (`_sfetrx4_stream_send`). The samples are converted to the wire
    /// format while the transport lends the buffer, through a bounded scratch; the packet
    /// is then announced at the core's registers unless the transport framed it (USB).
    /// Returns the core's telemetry when the transport had status words for the buffer.
    ///
    /// On a timeout, libusdr reads the core's status words for its log; so does this.
    pub(crate) fn send<S: HostSample>(
        &mut self,
        channels: &[&[S]],
        time: Option<StreamTime>,
        timeout: Duration,
    ) -> Result<Option<TxTelemetry>, Error> {
        self.check_send(channels, time)?;
        let Some(stream) = self.streams.as_mut().and_then(|group| group.tx.as_mut()) else {
            return Err(Error::NotStreaming);
        };
        let samples = channels.first().map_or(0, |channel| channel.len());
        let stamp = time.map(StreamTime::samples);

        let wire_bytes = samples * stream.sample_bytes;
        let converter = &stream.converter;
        let mut status = None;
        let mut written = 0;
        let bus = self.bus.as_mut();
        let sent = bus.tx_stream_send(timeout, stamp, &mut |buffer, words| {
            status = words;
            let Some(wire) = buffer.get_mut(..wire_bytes) else {
                return 0;
            };
            convert(converter, channels, wire);
            written = wire_bytes;
            wire_bytes
        });
        let commit = match sent {
            Ok(commit) => commit,
            Err(BusError::Timeout) => {
                let words = fpga::read_tx_status(bus)?;
                warn!(status = ?words, "no TX buffer freed in time");
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
                    push.push(bus, bytes, count, stamp)?;
                }
                None => fpga::push_tx(bus, count, stamp)?,
            }
        }
        Ok(telemetry)
    }
}

/// Converts `channels`' samples into `wire`, exactly their wire size, at most
/// [`CHUNK_SAMPLES`] at a time through a stack scratch per channel: chunked, libusdr's
/// one-call conversion of the packet.
fn convert<S: HostSample>(converter: &Converter, channels: &[&[S]], wire: &mut [u8]) {
    const { assert!(S::BYTES <= MAX_HOST_SAMPLE_BYTES) };
    let mut scratch = [[0; CHUNK_SAMPLES * MAX_HOST_SAMPLE_BYTES]; MAX_CHANNELS];
    let samples = channels.first().map_or(0, |channel| channel.len());
    let wire_sample_bytes = wire.len() / samples;
    let outs = wire.chunks_mut(CHUNK_SAMPLES * wire_sample_bytes);
    for (start, out) in (0..samples).step_by(CHUNK_SAMPLES).zip(outs) {
        let len = CHUNK_SAMPLES.min(samples - start);
        for (channel, host) in channels.iter().zip(&mut scratch) {
            let chunk = &channel[start..start + len];
            for (sample, raw) in chunk.iter().zip(host.chunks_exact_mut(S::BYTES)) {
                sample.to_le(raw);
            }
        }
        let hosts = scratch.each_ref().map(|host| &host[..len * S::BYTES]);
        converter
            .convert(&hosts[..channels.len()], &mut [out])
            .expect("invariant: the converter takes one buffer per channel in, one out");
    }
}

#[cfg(test)]
mod tests {
    use num_complex::Complex;

    use super::*;
    use crate::dsp::SampleFormat;

    #[test]
    fn chunked_conversion_equals_one_call() {
        let samples: Vec<Complex<f32>> = (0..CHUNK_SAMPLES * 2 + 3)
            .map(|n| {
                let x = f32::from(u16::try_from(n % 1000).expect("small")) / 1000.0;
                Complex::new(x, -x)
            })
            .collect();
        let converter =
            Converter::new(SampleFormat::Cf32, SampleFormat::Ci16, 1, 1).expect("libusdr has it");
        let host: Vec<u8> = samples
            .iter()
            .flat_map(|s| [s.re.to_le_bytes(), s.im.to_le_bytes()].concat())
            .collect();
        let mut whole = vec![0; samples.len() * 4];
        converter
            .convert(&[&host], &mut [&mut whole])
            .expect("one buffer each side");
        let mut chunked = vec![0; samples.len() * 4];
        convert(&converter, &[&samples], &mut chunked);
        assert_eq!(
            chunked, whole,
            "chunk boundaries do not change the wire bytes"
        );
    }

    #[test]
    fn chunked_two_channel_conversion_equals_one_call() {
        let channel = |sign: i16| -> Vec<Complex<i16>> {
            (0..CHUNK_SAMPLES * 2 + 3)
                .map(|n| {
                    let n = i16::try_from(n).expect("small");
                    Complex::new(sign * n, -sign * n)
                })
                .collect()
        };
        let (zero, one) = (channel(1), channel(-1));
        let bytes = |samples: &[Complex<i16>]| -> Vec<u8> {
            samples
                .iter()
                .flat_map(|s| [s.re.to_le_bytes(), s.im.to_le_bytes()].concat())
                .collect()
        };
        let converter =
            Converter::new(SampleFormat::Ci16, SampleFormat::Ci16, 2, 1).expect("libusdr has it");
        let mut whole = vec![0; zero.len() * 8];
        converter
            .convert(&[&bytes(&zero), &bytes(&one)], &mut [&mut whole])
            .expect("two buffers in, one out");
        let mut chunked = vec![0; zero.len() * 8];
        convert(&converter, &[&zero, &one], &mut chunked);
        assert_eq!(
            chunked, whole,
            "chunk boundaries do not change the interleaved wire bytes"
        );
    }

    #[test]
    fn one_channel_mutes_the_lane_it_does_not_use_and_two_mute_neither() {
        assert_eq!(
            TxLaneUse::lane_use(TxChannels::Channel0, None),
            (TxMute::B, false)
        );
        assert_eq!(
            TxLaneUse::lane_use(TxChannels::Channel1, None),
            (TxMute::A, true)
        );
        assert_eq!(
            TxLaneUse::lane_use(TxChannels::Both, None),
            (TxMute::Neither, false)
        );
        // A remap keeps "neither", and moves any other one-channel mute to the idle lane
        // (`fe_old_tx_mute = swap ? 1 : 2`); two channels keep theirs.
        assert_eq!(
            TxLaneUse::lane_use(TxChannels::Channel1, Some(TxMute::Neither)),
            (TxMute::Neither, true)
        );
        assert_eq!(
            TxLaneUse::lane_use(TxChannels::Channel1, Some(TxMute::Both)),
            (TxMute::A, true)
        );
        assert_eq!(
            TxLaneUse::lane_use(TxChannels::Channel0, Some(TxMute::A)),
            (TxMute::B, false)
        );
        assert_eq!(
            TxLaneUse::lane_use(TxChannels::Both, Some(TxMute::Both)),
            (TxMute::Both, false)
        );
    }
}
