//! The RX and TX streams: libusdr's `/ll/srx/0` and `/ll/stx/0` in `ci16`, created, synced,
//! read, written and destroyed as `u3_limesdr.c` (`limesdr_device_create_stream`,
//! `dev_limesdr_initialize`, `limesdr_device_stream_sync`, `limesdr_device_unregister_stream`)
//! and `ipblks/streams/stream_limesdr.c` do. This gateware is not the uSDR's stream engine,
//! so nothing here is `sdr-stream`'s.

use std::time::Duration;

use super::Device;
use crate::control::BusError;
use crate::error::Error;
use crate::portable::framing::{
    BURST_BYTES, BURST_SAMPLES, RxDeframer, TX_SAMPLE_MULTIPLE, frame_tx, rx_bursts,
    tx_frame_bytes, tx_plan,
};
use crate::portable::sync::StreamSync;

/// The latest time a send can carry: libusdr takes a time whose top bit is set as none
/// (`((int64_t)timestamp) < 0`).
const MAX_TX_TIME: u64 = i64::MAX as u64;

/// One packet [`Device::receive`] delivered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RxPacket {
    /// Samples written to the front of the buffer: always the stream's packet size.
    pub samples: usize,

    /// The gateware's time of the packet's first sample, in samples since the stream
    /// sync last reset it, zero samples filled in for lost ones included.
    pub time: u64,

    /// Whole packets lost since the stream was created, as libusdr counts them.
    pub lost_packets: u64,
}

/// A created RX stream (libusdr's `stream_limesdr_t` for stream 0).
#[derive(Debug)]
pub(super) struct RxStream {
    /// The deframing state.
    deframer: RxDeframer,
    /// The block in hand, which the deframer may have partly consumed.
    block: Vec<u8>,
    /// Samples per packet.
    samples_per_packet: usize,
}

/// A created TX stream (libusdr's `stream_limesdr_t` for stream 1).
#[derive(Debug)]
pub(super) struct TxStream {
    /// The most samples one send takes: the block's bursts' worth.
    max_samples: usize,
}

impl Device {
    /// Creates the RX stream with `samples_per_packet` samples per packet, in `ci16`
    /// (libusdr's `usdr_dms_create` of `/ll/srx/0`): the LMS7002M prepared to stream as
    /// [`Self::prepare_streaming`] does, then the transport's stream set up for blocks of
    /// whole bursts (a packet's, or two fifths of them when it is not whole bursts, as
    /// libusdr sizes them). Samples flow once [`Self::start_streams`] has run.
    ///
    /// # Errors
    ///
    /// Before any exchange: [`Error::AlreadyStreaming`] with an RX stream;
    /// [`Error::UnsupportedPacketSize`] for 0 samples or a block over 4 GiB. Then
    /// [`Self::prepare_streaming`]'s errors, or [`Error::Stream`] when the transport
    /// cannot set the stream up.
    pub fn create_rx_stream(&mut self, samples_per_packet: u32) -> Result<(), Error> {
        if self.rx_stream.is_some() {
            return Err(Error::AlreadyStreaming);
        }
        let bursts = rx_bursts(samples_per_packet);
        let deframer = RxDeframer::new(samples_per_packet, bursts);
        if samples_per_packet == 0 || bursts.checked_mul(BURST_BYTES).is_none() {
            return Err(Error::UnsupportedPacketSize(samples_per_packet));
        }
        self.prepare_streaming()?;
        let block_bytes = deframer.block_bytes();
        self.port
            .rx_stream_open(block_bytes)
            .map_err(Error::Stream)?;
        self.rx_stream = Some(RxStream {
            deframer,
            block: vec![0; size(block_bytes)],
            samples_per_packet: size(samples_per_packet),
        });
        Ok(())
    }

    /// Creates the TX stream with `samples_per_packet` samples per packet, in `ci16`
    /// (libusdr's `usdr_dms_create` of `/ll/stx/0`): 0 asks for libusdr's 32 640, fewer
    /// than 4096 get 4096. The LMS7002M is prepared to stream as [`Self::prepare_streaming`]
    /// does, then the transport's stream set up for blocks of as many bursts as a packet
    /// fills.
    ///
    /// # Errors
    ///
    /// Before any exchange: [`Error::AlreadyStreaming`] with a TX stream;
    /// [`Error::UnsupportedPacketSize`] for a packet not a multiple of 4 samples or a
    /// block over 4 GiB. Then [`Self::prepare_streaming`]'s errors, or [`Error::Stream`]
    /// when the transport cannot set the stream up.
    pub fn create_tx_stream(&mut self, samples_per_packet: u32) -> Result<(), Error> {
        if self.tx_stream.is_some() {
            return Err(Error::AlreadyStreaming);
        }
        let bursts = tx_plan(samples_per_packet)
            .filter(|bursts| bursts.checked_mul(BURST_BYTES).is_some())
            .ok_or(Error::UnsupportedPacketSize(samples_per_packet))?;
        self.prepare_streaming()?;
        self.port
            .tx_stream_open(bursts * BURST_BYTES)
            .map_err(Error::Stream)?;
        self.tx_stream = Some(TxStream {
            max_samples: size(bursts * BURST_SAMPLES),
        });
        Ok(())
    }

    /// Starts streaming as libusdr's stream sync does for any sync type but `"off"`
    /// (`limesdr_setup_stream`, in SISO SDR with 16-bit samples): chip 0 selected,
    /// streaming stopped, the timestamp reset, the channel-A TX DC correction set and the
    /// DC correctors enabled for the running paths, channel 0's stream mode set, then
    /// streaming started. Once started, a second start does nothing, as in libusdr.
    ///
    /// Divergences: libusdr ignores the setup's result and counts the streams started
    /// even when it failed; here a failure is reported and a later start tries again.
    ///
    /// # Errors
    ///
    /// [`Error::StillStreaming`] when streaming reads as running after it was stopped
    /// (libusdr's debug build refuses the timestamp reset then, `-EPERM`); [`Error::Bus`].
    pub fn start_streams(&mut self) -> Result<(), Error> {
        if self.streams_started {
            return Ok(());
        }
        let mut setup = StreamSync::setup(&self.control);
        self.run(&mut setup)?;
        if setup.refused() {
            return Err(Error::StillStreaming);
        }
        self.streams_started = true;
        Ok(())
    }

    /// Stops streaming as libusdr's stream sync does for `"off"` (`limesdr_disable_stream`):
    /// chip 0 selected, then streaming stopped, started or not.
    ///
    /// # Errors
    ///
    /// [`Error::Bus`]; libusdr ignores it.
    pub fn stop_streams(&mut self) -> Result<(), Error> {
        self.streams_started = false;
        self.run(&mut StreamSync::disable())
    }

    /// Receives one packet into the front of `buffer`, each sample I then Q, waiting up to
    /// `timeout` for each block it needs (`_limestr_stream_recv` into `ci16`). A packet
    /// may span blocks; the rest of a block is kept for the next receive. Samples a burst's
    /// time says were lost are filled in as zeroes, whole lost packets dropped.
    ///
    /// Divergences: a block is copied out of the transport's buffer and released at once,
    /// where libusdr holds the buffer until its samples are consumed; a block shorter than
    /// the stream's is refused, where libusdr reads past it.
    ///
    /// # Errors
    ///
    /// Before any exchange: [`Error::NotStreaming`]; [`Error::BufferTooSmall`] for a
    /// buffer shorter than a packet. Then [`Error::Timeout`] when no block arrives in time,
    /// [`Error::Stream`] for another transport failure, or [`Error::ShortRxBlock`]. The
    /// samples written before a failure stay in `buffer`.
    pub fn receive(
        &mut self,
        buffer: &mut [[i16; 2]],
        timeout: Duration,
    ) -> Result<RxPacket, Error> {
        let Some(stream) = self.rx_stream.as_mut() else {
            return Err(Error::NotStreaming);
        };
        let needed = stream.samples_per_packet;
        if buffer.len() < needed {
            return Err(Error::BufferTooSmall {
                needed,
                got: buffer.len(),
            });
        }
        let port = &mut self.port;
        let time = stream
            .deframer
            .receive(&mut stream.block, buffer, |block| {
                let mut got = 0;
                port.rx_stream_recv(timeout, &mut |data| {
                    got = data.len();
                    if let Some(data) = data.get(..block.len()) {
                        block.copy_from_slice(data);
                    }
                })
                .map_err(|error| match error {
                    BusError::Timeout => Error::Timeout,
                    other => Error::Stream(other),
                })?;
                if got < block.len() {
                    return Err(Error::ShortRxBlock {
                        expected: block.len(),
                        got,
                    });
                }
                Ok(())
            })?;
        Ok(RxPacket {
            samples: needed,
            time,
            lost_packets: stream.deframer.overruns(),
        })
    }

    /// Sends `samples`, each I then Q, to play from `time` in samples or, with `None`, as
    /// soon as they reach the gateware (`_limestr_stream_send` from `ci16`): one block of
    /// a burst per 1020 samples, each burst's header carrying its length and its first
    /// sample's time.
    ///
    /// # Errors
    ///
    /// Before any exchange: [`Error::NotStreaming`]; [`Error::SendSize`] for more samples
    /// than the stream's block holds or a count not a multiple of 4;
    /// [`Error::UnsupportedTimestamp`] for a time at or after 2^63, which libusdr would
    /// take for none. Then [`Error::Timeout`] when no buffer frees in time, or
    /// [`Error::Stream`] for another transport failure or a buffer shorter than the block.
    pub fn send(
        &mut self,
        samples: &[[i16; 2]],
        time: Option<u64>,
        timeout: Duration,
    ) -> Result<(), Error> {
        let Some(stream) = self.tx_stream.as_ref() else {
            return Err(Error::NotStreaming);
        };
        let multiple = size(TX_SAMPLE_MULTIPLE);
        if samples.len() > stream.max_samples || samples.len() % multiple != 0 {
            return Err(Error::SendSize {
                max: stream.max_samples,
                multiple,
                got: samples.len(),
            });
        }
        if let Some(time) = time.filter(|&time| time > MAX_TX_TIME) {
            return Err(Error::UnsupportedTimestamp(time));
        }
        let needed = tx_frame_bytes(samples.len());
        let mut written = 0;
        self.port
            .tx_stream_send(timeout, time, &mut |buffer| {
                let Some(out) = buffer.get_mut(..needed) else {
                    return 0;
                };
                written = frame_tx(samples, time, out);
                written
            })
            .map_err(|error| match error {
                BusError::Timeout => Error::Timeout,
                other => Error::Stream(other),
            })?;
        if written != needed {
            return Err(Error::Stream(BusError::Unsupported(
                "TX buffer shorter than the block",
            )));
        }
        Ok(())
    }

    /// Destroys the RX stream as libusdr's `usdr_dms_destroy` does on this family
    /// (`limesdr_device_unregister_stream`): it is forgotten, nothing reaches the board or
    /// the transport, and streaming runs on as the last sync left it.
    ///
    /// # Errors
    ///
    /// [`Error::NotStreaming`] without an RX stream.
    pub fn destroy_rx_stream(&mut self) -> Result<(), Error> {
        self.rx_stream.take().map(drop).ok_or(Error::NotStreaming)
    }

    /// Destroys the TX stream, as [`Self::destroy_rx_stream`] does the RX one.
    ///
    /// # Errors
    ///
    /// [`Error::NotStreaming`] without a TX stream.
    pub fn destroy_tx_stream(&mut self) -> Result<(), Error> {
        self.tx_stream.take().map(drop).ok_or(Error::NotStreaming)
    }
}

/// A 32-bit count as a buffer length.
fn size(count: u32) -> usize {
    usize::try_from(count).expect("invariant: a 32-bit count fits in usize")
}
