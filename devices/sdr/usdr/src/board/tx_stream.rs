//! TX streaming: libusdr's `/ll/stx/0` stream on channel 0, created, started, fed, muted
//! and destroyed as `usdr_device_m2_lm6_1_create_stream`'s TX branch, `usdr_dms_op`,
//! `usdr_dms_send_stat`, the stream option `"mute"` and `unregister_stream` do it
//! (`device/m2_lm6_1/m2_lm6_1.c`; the stream engine in
//! `ipblks/streams/stream_sfetrx4_dma32.c` and `sfe_tx_4.c`). The group around it is in
//! `stream.rs`.
//!
//! Creating the stream powers the transmitter, checks the gateware is new enough, holds
//! the sync unit stopped, asks the transport for its buffers, turns the constant generator
//! off and writes the IQ correction again. Each send converts the host's samples into a
//! transport buffer and announces it to the TX core with its time. Only the original TX
//! core is ported; the gateware's extended core is refused.

use std::time::Duration;

use rsl_deps::tracing::{debug, warn};

use super::Board;
use super::stream::{CHUNK_SAMPLES, MAX_HOST_SAMPLE_BYTES};
use crate::dsp::Converter;
use crate::error::Error;
use crate::fpga::{self, MAX_TX_PACKET_BYTES, MAX_TX_TIMESTAMP, Phy, SyncTrigger, TxCoreStatus};
use crate::lowlevel::{Bus, BusError};
use crate::stream::{
    HostFormat, HostSample, StreamTime, TxChannels, TxConfig, TxMute, TxTelemetry, WireFormat,
};

/// The oldest gateware build libusdr streams TX from (`MINIM_FWID_COMPAT`), as the FPGA's
/// `USR_ACCESS2` stamps it.
const MIN_TX_GATEWARE: u32 = 0xd2b1_0c09;

/// A created TX stream's bookkeeping (libusdr's `stream_sfetrx_dma32_t` for TX).
#[derive(Debug)]
pub(crate) struct TxStream {
    /// Samples per packet: the most one send takes.
    samples_per_packet: u32,

    /// The host buffers' format.
    host: HostFormat,

    /// The channels, one host buffer each.
    channels: TxChannels,

    /// Host to wire conversion (libusdr's `tf_data`).
    converter: Converter,

    /// The lanes muted (`fe_old_tx_mute`), which starting the core applies.
    mute: TxMute,

    /// Bursts the front end dropped so far, extended past its 8-bit counter (`fe_drop`).
    front_end_drops: u64,

    /// Bursts the DMA dropped so far, likewise (`dma_drop`).
    dma_drops: u64,
}

impl TxStream {
    /// Starts the TX core with the stream's mute flags (`_sfetrx4_op` starting TX).
    pub(super) fn start(&self, bus: &mut dyn Bus) -> Result<(), Error> {
        fpga::start_tx(bus, self.mute.bits())
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

impl Board {
    /// Refuses, before any bus access, a TX stream the ported core cannot take: the
    /// gateware's extended TX core, whose setup is not ported, and any wire format but
    /// `ci16`, which libusdr refuses only after powering the transmitter.
    pub(super) fn check_tx_stream(&self, config: TxConfig) -> Result<(), Error> {
        if self.chains.extended_tx_core {
            return Err(Error::UnsupportedTxCore);
        }
        if config.wire != WireFormat::Ci16 {
            return Err(Error::UnsupportedWireFormat(config.wire));
        }
        Ok(())
    }

    /// libusdr's stream create for TX on the original core (`initialize_stream_tx_32`),
    /// joining the group made for `sync`. Call [`Board::check_tx_stream`] first.
    ///
    /// Divergence on a failed read only: libusdr logs a failed gateware read and compares
    /// whatever it holds; this returns the failure.
    pub(super) fn create_tx_stream(
        &mut self,
        config: TxConfig,
        sync: SyncTrigger,
    ) -> Result<(), Error> {
        let TxConfig {
            samples_per_packet,
            wire,
            host,
            channels,
        } = config;
        let converter = Converter::new(host.sample_format(), wire.sample_format(), 1, 1)
            .expect("invariant: libusdr converts every host format to ci16");
        // `usdr_rfic_streaming_up`: libusdr takes its wake-up wait even when powering fails.
        let waking = !self.tx_running;
        self.tx_running = true;
        let powered = self.set_tx_power(true);
        if waking {
            self.bus.sleep(Duration::from_millis(1));
        }
        powered?;
        let build = self.identity()?.gateware_build;
        if build_time(build) < build_time(MIN_TX_GATEWARE) {
            return Err(Error::OutdatedGateware(build));
        }
        let bus = self.bus.as_mut();
        fpga::sync(bus, SyncTrigger::Stop)?;
        // libusdr fails a packet over the core's FIFO here; an empty one fails at the
        // transport's setup, which this never asks for.
        let block_bytes = samples_per_packet
            .checked_mul(fpga::TX_SAMPLE_BYTES)
            .filter(|&bytes| bytes != 0 && bytes <= MAX_TX_PACKET_BYTES)
            .ok_or(Error::UnsupportedPacketSize(samples_per_packet))?;
        bus.tx_stream_open(block_bytes).map_err(Error::Stream)?;
        self.group(sync).tx = Some(TxStream {
            samples_per_packet,
            host,
            channels,
            converter,
            mute: TxMute::B,
            front_end_drops: 0,
            dma_drops: 0,
        });
        // `usdr_tx_gen_set(.., false, ..)`, then `usdr_txupdate_cal`.
        Phy::set_tx_constant(self.bus.as_mut(), None)?;
        Phy::Tx.set_iq_correction(self.bus.as_mut(), self.tx.iq_imbalance.tx_terms())?;
        debug!(samples_per_packet, ?host, block_bytes, "TX stream created");
        Ok(())
    }

    /// Destroys the TX stream (`unregister_stream`): powers the transmitter down, then
    /// `_sfetrx4_destroy` clears the core's control register and releases the transport.
    /// Every step is attempted; the first failure is returned.
    pub(super) fn destroy_tx_stream(&mut self) -> Result<(), Error> {
        // `usdr_rfic_streaming_down`.
        self.tx_running = false;
        let powered_down = self.set_tx_power(false);
        let halted = fpga::stop_tx(self.bus.as_mut());
        let closed = self.bus.tx_stream_close().map_err(Error::Stream);
        debug!("TX stream stopped");
        powered_down.and(halted).and(closed)
    }

    /// Mutes the core's lanes as `mute` says (`_sfetrx4_option_set`'s `"mute"` on the
    /// original core); starting the stream applies it too.
    pub(crate) fn set_tx_mute(&mut self, mute: TxMute) -> Result<(), Error> {
        let Some(stream) = self.streams.as_mut().and_then(|group| group.tx.as_mut()) else {
            return Err(Error::NotStreaming);
        };
        stream.mute = mute;
        fpga::update_tx_flags(self.bus.as_mut(), mute.bits())
    }

    /// Sends one packet: the front of each channel's buffer, all of it, waiting up to
    /// `timeout` for a transport buffer, to play at `time` or, with `None`, as soon as it
    /// reaches the core (`_sfetrx4_stream_send`). The samples are converted to the wire
    /// format while the transport lends the buffer, through a bounded scratch. Returns
    /// the core's telemetry when the transport had status words for the buffer.
    ///
    /// On a timeout, libusdr reads the core's status words for its log; so does this.
    pub(crate) fn send<S: HostSample>(
        &mut self,
        channels: &[&[S]],
        time: Option<StreamTime>,
        timeout: Duration,
    ) -> Result<Option<TxTelemetry>, Error> {
        let Some(stream) = self.streams.as_mut().and_then(|group| group.tx.as_mut()) else {
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
            unreachable!("invariant: TX streams have one channel until channel maps grow");
        };
        let max = usize::try_from(stream.samples_per_packet)
            .expect("invariant: a packet size fits in usize");
        if samples.is_empty() || samples.len() > max {
            return Err(Error::SendSize {
                max,
                got: samples.len(),
            });
        }
        let stamp = time
            .map(|time| match time.samples() {
                samples if samples <= MAX_TX_TIMESTAMP => Ok(samples),
                _ => Err(Error::UnsupportedTimestamp(time)),
            })
            .transpose()?;

        let wire_bytes = samples.len() * usize::try_from(fpga::TX_SAMPLE_BYTES).expect("4");
        let converter = &stream.converter;
        let mut status = None;
        let mut written = 0;
        let bus = self.bus.as_mut();
        let sent = bus.tx_stream_send(timeout, &mut |buffer, words| {
            status = words;
            let Some(wire) = buffer.get_mut(..wire_bytes) else {
                return 0;
            };
            convert(converter, samples, wire);
            written = wire_bytes;
            wire_bytes
        });
        match sent {
            Ok(()) => {}
            Err(BusError::Timeout) => {
                let words = fpga::read_tx_status(bus)?;
                warn!(status = ?words, "no TX buffer freed in time");
                return Err(Error::Timeout);
            }
            Err(other) => return Err(Error::Stream(other)),
        }
        if written != wire_bytes {
            return Err(Error::Stream(BusError::Unsupported(
                "TX buffer shorter than the stream's packet",
            )));
        }
        let telemetry = status.map(|words| stream.account(words));
        let count = u32::try_from(samples.len()).expect("invariant: at most one packet");
        fpga::push_tx(bus, count, stamp)?;
        Ok(telemetry)
    }
}

/// Converts `samples` into `wire`, exactly their wire size, at most [`CHUNK_SAMPLES`] at a
/// time through a stack scratch: chunked, libusdr's one-call conversion of the packet.
fn convert<S: HostSample>(converter: &Converter, samples: &[S], wire: &mut [u8]) {
    const { assert!(S::BYTES <= MAX_HOST_SAMPLE_BYTES) };
    let mut scratch = [0; CHUNK_SAMPLES * MAX_HOST_SAMPLE_BYTES];
    let wire_sample_bytes = wire.len() / samples.len();
    let chunks = samples
        .chunks(CHUNK_SAMPLES)
        .zip(wire.chunks_mut(CHUNK_SAMPLES * wire_sample_bytes));
    for (chunk, out) in chunks {
        let host = &mut scratch[..chunk.len() * S::BYTES];
        for (sample, raw) in chunk.iter().zip(host.chunks_exact_mut(S::BYTES)) {
            sample.to_le(raw);
        }
        converter
            .convert(&[&*host], &mut [out])
            .expect("invariant: the converter takes one buffer each side");
    }
}

/// libusdr's `get_xilinx_rev_h`: a Xilinx `USR_ACCESS` build stamp (day, month, year
/// from 2000, hour, minute, second in bits 31:27, 26:23, 22:17, 16:12, 11:6, 5:0) as a
/// number that sorts by build time.
fn build_time(stamp: u32) -> u64 {
    let field = |shift: u32, bits: u32| u64::from(stamp >> shift & ((1 << bits) - 1));
    let (day, month, year) = (field(27, 5), field(23, 4), field(17, 6));
    let (hour, minute, second) = (field(12, 5), field(6, 6), field(0, 6));
    second + (minute + (hour + (day + (month + (2000 + year) * 100) * 100) * 100) * 100) * 100
}

#[cfg(test)]
mod tests {
    use num_complex::Complex;

    use super::*;
    use crate::dsp::SampleFormat;

    #[test]
    fn builds_sort_by_date_not_by_their_raw_bits() {
        let minimum = build_time(MIN_TX_GATEWARE);
        assert!(
            build_time(MIN_TX_GATEWARE - 1) < minimum,
            "a second earlier"
        );
        // Day 1 of a later year: a smaller word, a later build.
        let next_year_day_one = 1 << 27 | ((MIN_TX_GATEWARE >> 17 & 0x3f) + 1) << 17;
        assert!(next_year_day_one < MIN_TX_GATEWARE);
        assert!(build_time(next_year_day_one) > minimum);
    }

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
        convert(&converter, &samples, &mut chunked);
        assert_eq!(
            chunked, whole,
            "chunk boundaries do not change the wire bytes"
        );
    }
}
