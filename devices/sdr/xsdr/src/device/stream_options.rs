//! The streams' options beyond create, send, receive and stop: the sync unit's start modes
//! (`usdr_dms_sync`'s types, `sfetrx4_stream_sync`), the TX stream's `"mute"` and
//! `"chmap"` options and the RX stream's `"throttle"` and `"ready"` (`_sfetrx4_option_set`
//! in `ipblks/streams/stream_sfetrx4_dma32.c`, `/dm/sdr/0/rfe/throttle`), and the RX
//! `LimeLight` map's A/B swap (`/dm/sdr/0/rxdsp/swapab`, `xsdr_rfic_streaming_xflags`).

use lms7002m::ctrl::{Direction, Frontend, StreamFlags};
use lms7002m::{LmlMap, Mac};
use sdr_stream::{EngineSequence, SyncTrigger};

use super::{Device, run};
use crate::error::Error;
use crate::portable::frontend::XsdrFrontend;
use crate::portable::step::Step;

/// How a stream starts once started: libusdr's `usdr_dms_sync` types.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum StreamSync {
    /// The stream runs as soon as it starts (`"none"`: the sync unit free-runs).
    #[default]
    None,

    /// The stream starts on its own front end's activity (`"all"` with one stream: RX's
    /// ADC, `ST_ADCACT`, or TX's DAC, `ST_DACACT`).
    All,

    /// Either front end's activity (`"any"`, `ST_ADCORDACACT`).
    Any,

    /// The RX front end's activity, whichever stream starts (`"rx"`, `ST_ADCACT`).
    RxActivity,

    /// The TX front end's activity (`"tx"`, `ST_DACACT`).
    TxActivity,

    /// A pulse at the board's 1PPS input (`"1pps"`, also spelt `"sysref"`: `ST_ONEPPS`).
    OnePps,

    /// A SYSREF the sync unit generates itself (`"sysref+gen"`, `ST_ONEPPS |
    /// ST_SYSREF_GEN`).
    GeneratedSysref,
}

impl StreamSync {
    /// The sync unit's mode for a start of the `rx` or TX stream alone.
    const fn trigger(self, rx: bool) -> SyncTrigger {
        match self {
            Self::None => SyncTrigger::FreeRun,
            Self::All if rx => SyncTrigger::RxActivity,
            Self::All | Self::TxActivity => SyncTrigger::TxActivity,
            Self::Any => SyncTrigger::EitherActivity,
            Self::RxActivity => SyncTrigger::RxActivity,
            Self::OnePps => SyncTrigger::OnePps,
            Self::GeneratedSysref => SyncTrigger::GeneratedSysref,
        }
    }
}

/// Which TX lanes are muted: the TX stream's `"mute"` option, libusdr's two-bit mask (bit 0
/// lane A, bit 1 lane B). A channel-A stream is created with lane B muted, a channel-B
/// stream (its lanes swapped) with lane A, a two-channel stream with neither; the extended
/// TX core starts unmuted and takes the mask at its front end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxMute {
    /// Neither lane (0).
    Neither,

    /// Lane A (1).
    A,

    /// Lane B (2).
    B,

    /// Both lanes (3): the transmitter sends nothing.
    Both,
}

impl TxMute {
    /// libusdr's mask.
    const fn bits(self) -> u8 {
        match self {
            Self::Neither => 0,
            Self::A => 1,
            Self::B => 2,
            Self::Both => 3,
        }
    }
}

impl Device {
    /// Starts the RX stream as [`Self::start_rx_stream`] does, on `sync`: libusdr's
    /// `usdr_dms_sync(.., type, 1, [rx])`, then `USDR_DMS_START`.
    ///
    /// # Errors
    ///
    /// [`Error::NotStreaming`] without an RX stream, before any bus access; [`Error::Bus`].
    pub fn start_rx_stream_with(&mut self, sync: StreamSync) -> Result<(), Error> {
        if self.rx_stream.is_none() {
            return Err(Error::NotStreaming);
        }
        let mut start = EngineSequence::new().sync(sync.trigger(true)).run_rx(true);
        run(&mut *self.bus, &mut start)
    }

    /// Starts the TX stream as [`Self::start_tx_stream`] does, on `sync`: libusdr's
    /// `USDR_DMS_START`, then `usdr_dms_sync(.., type, 1, [tx])`.
    ///
    /// # Errors
    ///
    /// [`Error::NotStreaming`] without a TX stream, before any bus access; [`Error::Bus`].
    pub fn start_tx_stream_with(&mut self, sync: StreamSync) -> Result<(), Error> {
        let Some(stream) = self.tx_stream.as_ref() else {
            return Err(Error::NotStreaming);
        };
        let mut start =
            EngineSequence::start_lanes(false, Some(stream.lanes()), sync.trigger(false));
        run(&mut *self.bus, &mut start)
    }

    /// Mutes the TX stream's lanes as `mute` says (its `"mute"` option): on the original
    /// core the stream keeps it, writes it at once (`sfe_tx4_upd`) and starting the stream
    /// applies it too; on the extended core it goes to the front end's mute register
    /// (`exfe_tx4_mute`), and the start mutes nothing, as in libusdr.
    ///
    /// # Errors
    ///
    /// [`Error::NotStreaming`] without a TX stream, before any bus access; [`Error::Bus`].
    pub fn set_tx_stream_mute(&mut self, mute: TxMute) -> Result<(), Error> {
        let Some(stream) = self.tx_stream.as_mut() else {
            return Err(Error::NotStreaming);
        };
        let mut update = if stream.extended.is_some() {
            EngineSequence::new().extended_tx_mute(mute.bits().into())
        } else {
            stream.mute = mute.bits();
            EngineSequence::new().update_tx_lanes(stream.lanes())
        };
        run(&mut *self.bus, &mut update)
    }

    /// Moves the TX stream to `channels`, of the same count (its `"chmap"` option): on the
    /// original core its lanes swap as a new stream's would and, for one channel, a muted
    /// lane follows to the other (`fe_tx4_swap_ab_get`, `sfe_tx4_upd`); on the extended
    /// core its lane map is rewritten (`exfe_trx4_update_chmap`). The new use is kept
    /// before it is written, as libusdr keeps it.
    ///
    /// Divergence: libusdr reads only as many entries of a new map as the stream has
    /// channels, so a map of another count still applies; it is refused here. libusdr's
    /// swapped two-channel map (B then A) has no [`super::TxChannels`] and is not offered.
    ///
    /// # Errors
    ///
    /// [`Error::NotStreaming`] without a TX stream; [`Error::ChannelBuffers`] for
    /// `channels` of another count; both before any bus access. Then [`Error::Bus`].
    pub fn set_tx_stream_channels(&mut self, channels: super::TxChannels) -> Result<(), Error> {
        let Some(stream) = self.tx_stream.as_mut() else {
            return Err(Error::NotStreaming);
        };
        let (count, wanted) = (stream.channels.count(), channels.count());
        if count != wanted {
            return Err(Error::ChannelBuffers {
                expected: count,
                got: wanted,
            });
        }
        stream.channels = channels;
        let mut update = if stream.extended.is_some() {
            EngineSequence::new().extended_tx_map(channels.lane_map())
        } else {
            stream.swap = channels == super::TxChannels::B;
            if count == 1 && stream.mute != 0 {
                stream.mute = if stream.swap { 1 } else { 2 };
            }
            EngineSequence::new().update_tx_lanes(stream.lanes())
        };
        run(&mut *self.bus, &mut update)
    }

    /// Throttles the RX stream's bursts (`/dm/sdr/0/rfe/throttle`, its `"throttle"`
    /// option, `sfe_rx4_throttle`): with `Some((send, skip))` the front end delivers
    /// `send + 1` of every `send + skip + 2` bursts and drops the rest; `None` delivers
    /// every one.
    ///
    /// # Errors
    ///
    /// [`Error::NotStreaming`] without an RX stream (libusdr's `-EINVAL`), before any bus
    /// access; [`Error::Bus`].
    pub fn set_rx_throttle(&mut self, throttle: Option<(u8, u8)>) -> Result<(), Error> {
        if self.rx_stream.is_none() {
            return Err(Error::NotStreaming);
        }
        run(
            &mut *self.bus,
            &mut EngineSequence::new().throttle_rx(throttle),
        )
    }

    /// Tells the RX DMA engine the host is ready (the RX stream's `"ready"` option), as
    /// [`Self::receive`] does itself until the first packet.
    ///
    /// # Errors
    ///
    /// [`Error::NotStreaming`] without an RX stream, before any bus access; [`Error::Bus`].
    pub fn signal_rx_ready(&mut self) -> Result<(), Error> {
        if self.rx_stream.is_none() {
            return Err(Error::NotStreaming);
        }
        run(&mut *self.bus, &mut EngineSequence::new().signal_rx_ready())
    }

    /// Swaps the RX `LimeLight` map's channels A and B, or not (`/dm/sdr/0/rxdsp/swapab`,
    /// `xsdr_rfic_streaming_xflags(d, swap ? RFIC_SWAP_AB : 0, 0)`): both maps rewritten,
    /// RX's from the channels and flags it last streamed up with, A and B swapped if
    /// `swap`. As in libusdr the swap holds until the next rewrite, which a stream's
    /// change of channel count or a streaming up makes without it.
    ///
    /// # Errors
    ///
    /// [`Error::Bus`].
    pub fn swap_rx_channels(&mut self, swap: bool) -> Result<(), Error> {
        let pumped = self.rate.double_pump;
        let [rx, tx] = self.hw_channels.map(|channels| channels == 1 || pumped);
        let op = self.control.set_siso(RxSwap { swap }, rx, tx);
        self.run_control(op)
    }
}

/// The XSDR's board hooks with RX's A/B order flipped (`dsp_rxcfg`'s `RFIC_SWAP_AB`).
#[derive(Clone, Copy, Debug)]
struct RxSwap {
    /// Flip RX's A/B order.
    swap: bool,
}

impl Frontend for RxSwap {
    type Switch = Step;

    fn antenna_switch(&self, direction: Direction, position: u8) -> Step {
        XsdrFrontend.antenna_switch(direction, position)
    }

    fn lml_map(&self, rx: bool, channels: Mac, flags: StreamFlags) -> LmlMap {
        let flags = StreamFlags {
            swap_ab: flags.swap_ab ^ (rx && self.swap),
            ..flags
        };
        XsdrFrontend.lml_map(rx, channels, flags)
    }
}
