//! RX streaming: libusdr's `/ll/srx/0` `ci16` stream on channel 0, as the FFI `usdr` crate
//! creates, starts, reads and stops it (`usdr_device_m2_lm6_1_create_stream` and
//! `unregister_stream` in `device/m2_lm6_1/m2_lm6_1.c`; the stream engine in
//! `ipblks/streams/stream_sfetrx4_dma32.c`).
//!
//! Creating a stream powers the receiver, calibrates its DC offsets, configures the FPGA's
//! front end and DMA, and asks the transport for its buffers; starting runs the DMA and
//! front end; free-running releases the sync unit, after which blocks flow.

use std::time::Duration;

use num_complex::Complex;

use super::Board;
use crate::error::Error;
use crate::fpga::{self, BurstPlan, Phy, SyncMode};
use crate::lowlevel::BusError;

/// The frequency a stream tunes to if none was set, in Hz.
const DEFAULT_RX_HZ: u32 = 320_000_000;

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

/// A running RX stream's bookkeeping (libusdr's `stream_sfetrx_dma32_t`).
#[derive(Debug)]
pub(crate) struct RxStream {
    /// Samples per packet.
    samples_per_packet: u32,

    /// Samples per burst; a packet is a whole number of bursts.
    samples_per_burst: usize,

    /// Bytes per burst, any stub that pads it to whole words included.
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

impl Board {
    /// Creates, starts and free-runs the RX stream with `samples_per_packet` samples per
    /// packet: libusdr's create, `USDR_DMS_START` and `usdr_dms_sync(.., "none", ..)`.
    ///
    /// Intentional divergence: libusdr's create then reapplies each channel's LO as a
    /// baseband offset (`usdr_rfic_fe_set_freq(FE_FREQ_BB_RX, .., rx_raw.lo[i])`), which
    /// selects the mixer band, clamps the NCOs and bypasses the filter until the caller
    /// tunes again. That loop is left out, so a new stream keeps the band, NCOs and filter
    /// the caller set.
    pub(crate) fn start_rx_stream(&mut self, samples_per_packet: u32) -> Result<(), Error> {
        if self.rx_stream.is_some() {
            return Err(Error::AlreadyStreaming);
        }
        self.create_rx_stream(samples_per_packet)?;
        let bus = self.bus.as_mut();
        fpga::run_rx(bus, true)?;
        fpga::sync(bus, SyncMode::FreeRun)
    }

    /// libusdr's stream create for RX.
    fn create_rx_stream(&mut self, samples_per_packet: u32) -> Result<(), Error> {
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
        self.lms
            .calibrate_rx_dc(self.bus.as_mut(), Self::REFERENCE_HZ)?;
        // libusdr retunes the TX PLL here when a TX LO is set; TX is not ported, so it never
        // is.

        let bus = self.bus.as_mut();
        fpga::reset_rx_dma(bus)?;
        let plan = BurstPlan::for_packet(samples_per_packet)
            .ok_or(Error::UnsupportedPacketSize(samples_per_packet))?;
        fpga::configure_rx(bus, &plan)?;
        fpga::sync(bus, SyncMode::Stop)?;
        bus.rx_stream_open(plan.block_bytes())
            .map_err(Error::Stream)?;
        let size = |bytes: u32| usize::try_from(bytes).expect("invariant: a block fits in usize");
        self.rx_stream = Some(RxStream {
            samples_per_packet,
            samples_per_burst: size(plan.samples_per_burst()),
            bytes_per_burst: size(plan.bytes_per_burst()),
            block_bytes: size(plan.block_bytes()),
            packets: 0,
            next_sample: 0,
            lost_packets: 0,
        });
        // `usdr_rxupdate_cal`.
        Phy::Rx.set_iq_correction(self.bus.as_mut(), Phy::NO_IQ_IMBALANCE)
    }

    /// Samples per packet of the running stream.
    pub(crate) fn rx_packet_samples(&self) -> Option<u32> {
        self.rx_stream
            .as_ref()
            .map(|stream| stream.samples_per_packet)
    }

    /// Receives one packet into the front of `samples`, waiting up to `timeout`
    /// (`_sfetrx4_stream_recv`).
    ///
    /// Until the first packet arrives, each call first tells the DMA engine the host is
    /// ready, as libusdr does.
    pub(crate) fn receive(
        &mut self,
        samples: &mut [Complex<i16>],
        timeout: Duration,
    ) -> Result<RxPacket, Error> {
        let Some(stream) = self.rx_stream.as_mut() else {
            return Err(Error::NotStreaming);
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
        let (per_burst, burst_bytes, block_bytes) = (
            stream.samples_per_burst,
            stream.bytes_per_burst,
            stream.block_bytes,
        );
        bus.rx_stream_recv(timeout, &mut |block, words| {
            delivered = block.len();
            if block.len() >= block_bytes {
                decode(block, &mut samples[..needed], per_burst, burst_bytes);
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
        stream.lost_packets += lost;
        stream.next_sample += u64::from(stream.samples_per_packet) * lost;
        let first_sample = stream.next_sample;
        stream.next_sample += u64::from(stream.samples_per_packet);
        stream.packets += 1;
        Ok(RxPacket {
            samples: needed,
            first_sample,
            lost_packets: stream.lost_packets,
        })
    }

    /// Stops and destroys the RX stream: libusdr's `USDR_DMS_STOP`, then the stream
    /// destroy, which powers the receiver down and releases the transport's buffers. Every
    /// step is attempted; the first failure is returned.
    pub(crate) fn stop_rx_stream(&mut self) -> Result<(), Error> {
        if self.rx_stream.take().is_none() {
            return Err(Error::NotStreaming);
        }
        let stopped = fpga::run_rx(self.bus.as_mut(), false);
        // `usdr_rfic_streaming_down`.
        self.rx_running = false;
        let powered_down = self.set_rx_power(false);
        // `_sfetrx4_destroy`: a graceful stop, then the transport.
        let halted = fpga::run_rx(self.bus.as_mut(), false);
        let closed = self.bus.rx_stream_close().map_err(Error::Stream);
        stopped.and(powered_down).and(halted).and(closed)
    }
}

/// Copies a block's samples into `samples`, burst by burst: each burst holds
/// `samples_per_burst` samples of interleaved little-endian 16-bit I and Q, then any stub
/// padding it to `bytes_per_burst`.
///
/// Divergence from libusdr, invisible on the bus: its `ci16` pass-through copies the block
/// whole, stubs included, so a padded packet's samples land in the wrong places.
fn decode(
    block: &[u8],
    samples: &mut [Complex<i16>],
    samples_per_burst: usize,
    bytes_per_burst: usize,
) {
    let bursts = samples
        .chunks_mut(samples_per_burst)
        .zip(block.chunks(bytes_per_burst));
    for (burst, bytes) in bursts {
        for (sample, bytes) in burst.iter_mut().zip(bytes.chunks_exact(4)) {
            *sample = Complex::new(
                i16::from_le_bytes([bytes[0], bytes[1]]),
                i16::from_le_bytes([bytes[2], bytes[3]]),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_decode_as_little_endian_i_then_q() {
        let block = [0x01, 0x00, 0xff, 0xff, 0x00, 0x80, 0xff, 0x7f];
        let mut samples = [Complex::new(0, 0); 2];
        decode(&block, &mut samples, 2, 8);
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
        decode(&block, &mut samples, 1, 8);
        assert_eq!(samples, [Complex::new(1, 2), Complex::new(3, 4)]);
    }
}
