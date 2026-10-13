//! The radio as a caller sees it.

use std::fmt;
use std::time::Duration;

use num_complex::Complex;
use rsl_deps::tracing::warn;

use crate::board::{
    Board, Calibration, CalibrationReport, RxGain, RxIqCorrection, RxPacket, RxPath, RxThrottle,
    RxTrim, TxDcOffset, TxGain, TxIqCorrection, TxPath,
};
use crate::error::Error;
use crate::fpga::IqImbalance;
use crate::frontend::{
    FrontEndKind, FrontEndSelection, PciefeSetting, XMASS, XmassCalibrationPath,
};
use crate::identity::Identity;
use crate::lowlevel::Bus;
use crate::reference::ReferenceClock;
use crate::stream::{HostSample, RxMetadata, StreamTime, StreamsConfig, TxMute, TxTelemetry};
use crate::thermal::{CoolDown, ThermalPolicy};

/// A powered uSDR.
///
/// Dropping a `Device` powers the board down on a best-effort basis; call
/// [`Device::close`] to see power-down errors.
pub struct Device {
    /// The powered board, which owns the bus; `None` only once powered down.
    board: Option<Board>,

    /// Temperature limits in force.
    thermal: ThermalPolicy,
}

/// Options for opening a [`Device`]; see [`Device::builder`]. `'a` bounds the borrow of
/// a [`DeviceBuilder::wait_for_temperature`] callback.
pub struct DeviceBuilder<'a> {
    /// The hardware seam.
    bus: Box<dyn Bus>,

    /// Temperature limits to enforce.
    thermal: ThermalPolicy,

    /// Wait for a hot board to cool, reporting each reading, instead of failing.
    cool_down: Option<CoolDown<'a>>,

    /// Where the clocks' reference comes from.
    reference: ReferenceClock,

    /// Which front end to look for after power-up.
    front_end: FrontEndSelection,
}

impl Device {
    /// Opening options for a board reached through a custom [`Bus`]. Defaults:
    /// [`ThermalPolicy::WithinSpec`], and fail at once if the board is too hot.
    pub fn builder<'a>(bus: impl Bus + 'static) -> DeviceBuilder<'a> {
        DeviceBuilder {
            bus: Box::new(bus),
            thermal: ThermalPolicy::default(),
            cool_down: None,
            reference: ReferenceClock::Board,
            front_end: FrontEndSelection::Auto,
        }
    }

    /// Powers up a board reached through a custom [`Bus`] with the default options.
    ///
    /// # Errors
    ///
    /// As [`DeviceBuilder::open`].
    pub fn with_bus(bus: impl Bus + 'static) -> Result<Self, Error> {
        Self::builder(bus).open()
    }

    /// The board temperature in °C.
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub fn temperature(&mut self) -> Result<f32, Error> {
        self.board().temperature()
    }

    /// Moves the clocks' reference to `reference` (libusdr's `/dm/sdr/refclk/frequency` then
    /// `/dm/sdr/refclk/path`). As in libusdr, the dividers already programmed stay, so the
    /// sample clock and LO frequencies scale with the new reference until set again: set the
    /// sample rate, then the RX frequency. On revision 3, after powering up with an external
    /// reference the on-board oscillator stays disabled, so switching to
    /// [`ReferenceClock::Oscillator`] then reports a missing input clock.
    ///
    /// # Errors
    ///
    /// - [`Error::UnsupportedReferenceClock`] for an external reference outside
    ///   [`ReferenceClock::MIN_EXTERNAL_HZ`]..=[`ReferenceClock::MAX_EXTERNAL_HZ`], before
    ///   any bus access.
    /// - [`Error::ClockInputMissing`] if the Si5332 finds no clock on the new input. The
    ///   switch has still happened, and later plans use the new frequency.
    /// - Any bus failure, after which the clock state is unknown.
    pub fn set_reference_clock(&mut self, reference: ReferenceClock) -> Result<(), Error> {
        reference.check()?;
        self.board().set_reference_clock(reference)
    }

    /// The front end the power-up probe brought up, if any. On a uSDR with nothing on its
    /// connector this is the M.2-to-`PCIe` breakout without a DAC, as in libusdr: that board
    /// cannot be told apart from no board.
    #[must_use]
    pub fn front_end(&self) -> Option<FrontEndKind> {
        self.board.as_ref().and_then(Board::front_end)
    }

    /// Sets the front end's oscillator DAC to `code`, trimming its reference oscillator
    /// (libusdr's `/dm/sdr/0/dac_vctcxo`).
    ///
    /// # Errors
    ///
    /// [`Error::NoOscillatorDac`] without a `PCIe` front end or an M.2-to-`PCIe` breakout
    /// whose DAC answered (libusdr's `-EINVAL` and `-EACCES`); any bus failure.
    pub fn set_oscillator_dac(&mut self, code: u16) -> Result<(), Error> {
        self.board().set_oscillator_dac(code)
    }

    /// Applies one `PCIe` front-end control (libusdr's `/debug/hw/pciefe_cmd/0/reg` write).
    ///
    /// # Errors
    ///
    /// [`Error::FrontEndMissing`] without a `PCIe` front end; any bus failure.
    pub fn configure_pciefe(&mut self, setting: PciefeSetting) -> Result<(), Error> {
        self.board().configure_pciefe(setting)
    }

    /// Retunes the synchronization board's calibration LO to `hz` (libusdr's
    /// `/dm/sync/cal/freq`, on XMASS also `/dm/sdr/0/sync/cal/freq`), waiting for its
    /// clock synthesizer to lock: XMASS's calibration source (off below 3.1 MHz), or
    /// `SimpleSync`'s measurement LO on its outputs 0 to 3 (off below 3.5 MHz).
    ///
    /// On XMASS the request is kept even if it fails, as libusdr keeps it.
    ///
    /// # Errors
    ///
    /// [`Error::FrontEndMissing`] without XMASS or `SimpleSync`, before any bus access;
    /// [`Error::ClockPlan`] for a frequency the clock synthesizer cannot reach, before
    /// writing it; [`Error::ClockUnlocked`] if it does not lock; any bus failure.
    pub fn set_sync_calibration_frequency(&mut self, hz: u32) -> Result<(), Error> {
        self.board().set_sync_calibration_hz(hz)
    }

    /// The XMASS calibration LO last requested, in Hz (libusdr's `/dm/sync/cal/freq` read):
    /// 444 MHz after power-up.
    ///
    /// # Errors
    ///
    /// [`Error::FrontEndMissing`] without XMASS (`SimpleSync` has no such reading in
    /// libusdr).
    pub fn sync_calibration_frequency(&self) -> Result<u32, Error> {
        self.board
            .as_ref()
            .ok_or(Error::FrontEndMissing(XMASS))?
            .sync_calibration_hz()
    }

    /// Routes XMASS's calibration signal (libusdr's `/dm/sdr/0/sync/cal/path`).
    ///
    /// # Errors
    ///
    /// [`Error::FrontEndMissing`] without XMASS, before any bus access; any bus failure.
    pub fn set_sync_calibration_path(&mut self, path: XmassCalibrationPath) -> Result<(), Error> {
        self.board().set_sync_calibration_path(path)
    }

    /// Writes the gateware's clock-measurement control, raw (libusdr's
    /// `/dm/sdr/0/clkmeas` write, GPO `IGPO_CLKMEAS`). Its bits are not documented in the
    /// pinned libusdr; libusdr's GPSDO tool drives it.
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub fn set_clock_measurement(&mut self, control: u8) -> Result<(), Error> {
        self.board().set_clock_measurement(control)
    }

    /// Reads the gateware's clock-measurement word, raw (libusdr's `/dm/sdr/0/clkmeas`
    /// read: GPI word 24, which libusdr's header names `IGPI_USBC`). Its meaning is not
    /// documented in the pinned libusdr.
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub fn clock_measurement(&mut self) -> Result<u32, Error> {
        self.board().clock_measurement()
    }

    /// Writes the gateware's ATCRBS DSP core register, raw (libusdr's
    /// `/dm/sdr/0/core/atcrbs/reg`). The pinned libusdr documents no fields.
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub fn set_atcrbs_register(&mut self, value: u32) -> Result<(), Error> {
        self.board().set_atcrbs_register(value)
    }

    /// Reads the gateware's ATCRBS DSP core register, raw.
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub fn atcrbs_register(&mut self) -> Result<u32, Error> {
        self.board().atcrbs_register()
    }

    /// The TX DMA engine's four status words, raw, in register order (`STAT`, `STATM`,
    /// `STATTS`, `STAT_CPL`): libusdr's `/dm/debug/all`. The pinned libusdr documents no
    /// fields.
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub fn tx_dma_status(&mut self) -> Result<[u32; 4], Error> {
        self.board().tx_dma_status()
    }

    /// The board revision, its gateware's chains and build, read from the gateware now.
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub fn identity(&mut self) -> Result<Identity, Error> {
        self.board().identity()
    }

    /// Sets the RX sample rate, in samples per second.
    ///
    /// # Errors
    ///
    /// - [`Error::UnsupportedSampleRate`] outside 960 kS/s to 80 MS/s, or 1 to 80 MS/s on a
    ///   board with only one DSP chain.
    /// - [`Error::AlreadyStreaming`] while a stream group exists.
    /// - [`Error::ClockInputMissing`] if the Si5332 loses its reference while switching.
    /// - [`Error::PllUnlocked`], [`Error::PllFault`] or [`Error::UnsupportedFrequency`] if
    ///   the receiver is tuned below 230 MHz and retuning it for the new mixer LO fails.
    /// - Any bus failure.
    pub fn set_rx_sample_rate(&mut self, samples_per_second: u32) -> Result<(), Error> {
        self.board().set_rx_sample_rate(samples_per_second)
    }

    /// Tunes the receiver to `hz`.
    ///
    /// In the low band (below 230 MHz, unless [`Device::set_rx_path`] fixed the band) the
    /// board mixer lifts the signal into the LMS6002D's range, so the PLL's LO is about
    /// 300 MHz higher. If the PLL cannot lock at an LO below 250 MHz (in practice a request
    /// from 230 to 250 MHz), the receiver tunes the lowest LO that does and the NCO covers
    /// the rest, which needs a sample rate with room for the offset.
    ///
    /// # Errors
    ///
    /// - [`Error::PllUnlocked`] if the RX PLL cannot lock and no NCO fallback applies.
    /// - [`Error::UnsupportedFrequency`] for an LO below the PLL's 170 MHz floor: in the
    ///   low band before a sample rate sets the mixer LO, or below 170 MHz on a fixed
    ///   wide or high band. Also, in the low band, where `hz` plus the mixer LO does not
    ///   fit in a `u32` (libusdr's result there is undefined).
    /// - [`Error::PllFault`] if the LMS6002D reads back impossible PLL values.
    /// - Any bus failure.
    pub fn set_rx_frequency(&mut self, hz: u32) -> Result<(), Error> {
        self.board().set_rx_frequency(hz)
    }

    /// Creates the stream group `config` describes, stopped: libusdr's `usdr_dms_create`
    /// for each stream, RX first. Frequency and bandwidth can change while it exists; the
    /// sample rate cannot. Start it with [`Device::start_streams`].
    ///
    /// Creating the RX stream calibrates the receiver's DC offsets. If no frequency was set,
    /// the receiver tunes to 320 MHz first, as libusdr does. The calibration borrows the
    /// TX PLL; with a TX frequency set, it is tuned back to it, with any failure ignored
    /// as libusdr ignores it. Creating the TX stream powers the transmitter, checks the
    /// gateware's build, turns the constant generator off and writes the TX IQ correction
    /// again. Wire formats are converted to and from host formats inside each receive and
    /// send.
    ///
    /// # Errors
    ///
    /// Before any bus access:
    /// - [`Error::AlreadyStreaming`] if a group exists.
    /// - [`Error::NoStreams`] if `config` has no stream.
    /// - [`Error::UnsupportedSync`] for [`SyncMode::ExternalAll`]: libusdr's
    ///   `sfetrx4_stream_sync` has no `extall` case (it creates the stream, then fails
    ///   `usdr_dms_sync` with `-EINVAL`).
    /// - [`Error::UnsupportedTxCore`] for a TX stream on gateware with the extended TX
    ///   core, and [`Error::UnsupportedWireFormat`] for a TX wire format other than
    ///   [`WireFormat::Ci16`] (libusdr refuses it once the transmitter is powered).
    /// - [`Error::UnsupportedPacketSize`] for a TX packet of 0 samples, which libusdr takes
    ///   to mean the transport's largest buffer.
    ///
    /// Then:
    /// - [`Error::UnsupportedPacketSize`] for an RX packet the stream engine cannot split
    ///   into bursts in the wire format, or a TX packet past the core's 126976-byte FIFO
    ///   (31744 samples).
    /// - [`Error::OutdatedGateware`] for a TX stream on gateware older than libusdr's TX
    ///   streaming requires.
    /// - [`Error::Stream`] if the transport cannot set up its buffers, or does not stream.
    /// - [`Error::DmaEngineFault`] if the FPGA's DMA engine does not respond.
    /// - Tuning and calibration errors, as [`Device::set_rx_frequency`]; any bus failure.
    ///
    /// As in libusdr, a failure after a stream's transport set up its buffers leaves the
    /// group in place with the streams created so far, and a transmitter or receiver
    /// powered for a stream that failed stays powered: call [`Device::stop_streams`]
    /// before creating again. For USB, an ambiguous control transaction permanently
    /// invalidates the bus. Stopping still attempts to drain its RX buffers, but close/drop
    /// and reopen the device rather than retrying on that handle. Power-down writes may
    /// fail on the invalid bus.
    ///
    /// [`SyncMode::ExternalAll`]: crate::SyncMode::ExternalAll
    /// [`WireFormat::Ci16`]: crate::WireFormat::Ci16
    pub fn create_streams(&mut self, config: StreamsConfig) -> Result<(), Error> {
        self.board().create_streams(config)
    }

    /// Synchronises and starts the stream group. An RX-only group runs libusdr's
    /// `usdr_dms_sync` with the group's [`SyncMode`](crate::SyncMode), then
    /// `USDR_DMS_START`, as `dm_stream.h` prescribes; a group with TX starts RX, then TX
    /// (resetting its buffers and applying its [`TxMute`]), then syncs, as libusdr's own
    /// clients do, because the TX core's start writes the sync unit's register.
    ///
    /// # Errors
    ///
    /// [`Error::NotStreaming`] with no group; [`Error::AlreadyStreaming`] once started;
    /// any bus failure.
    pub fn start_streams(&mut self) -> Result<(), Error> {
        self.board().start_streams()
    }

    /// Receives one RX packet into the front of each channel's buffer (one buffer per
    /// channel of the stream), waiting up to `timeout`; the rest of each buffer is
    /// untouched. The buffers' element type must be the stream's
    /// [`HostFormat`](crate::HostFormat).
    ///
    /// Before [`Device::start_streams`], the sync unit holds the stream: the call tells
    /// the DMA engine the host is ready, as every call does until the first packet, then
    /// waits out `timeout`.
    ///
    /// Divergence from libusdr: a packet whose bursts end in padding (bursts that do not
    /// fill whole 8-byte words) arrives without it, where libusdr converts the padding
    /// into the samples.
    ///
    /// # Errors
    ///
    /// - [`Error::NotStreaming`] with no group.
    /// - [`Error::HostFormatMismatch`], [`Error::ChannelBuffers`] or
    ///   [`Error::BufferTooSmall`] for buffers that cannot hold the packet, before any bus
    ///   access.
    /// - [`Error::Timeout`] if no packet arrives in time.
    /// - [`Error::ShortRxBlock`] or [`Error::Stream`] if the transport fails; any bus
    ///   failure.
    pub fn receive_into<S: HostSample>(
        &mut self,
        channels: &mut [&mut [S]],
        timeout: Duration,
    ) -> Result<RxMetadata, Error> {
        self.board().receive_into(channels, timeout)
    }

    /// Sends one packet: each channel's whole buffer (one buffer per channel of the TX
    /// stream, 1 to its packet size samples, in its [`HostFormat`](crate::HostFormat)),
    /// waiting up to `timeout` for a free transport buffer. `time` is the TX core's
    /// timestamp for the first sample; `None` plays the packet as soon as it reaches the
    /// core (libusdr's all-ones time).
    ///
    /// As in libusdr, the core reports late or dropped bursts only through its counters
    /// ([`Device::send_with_stat`]); a send never fails for an underrun.
    ///
    /// # Errors
    ///
    /// - [`Error::NotStreaming`] with no TX stream.
    /// - [`Error::HostFormatMismatch`], [`Error::ChannelBuffers`], [`Error::SendSize`] or
    ///   [`Error::UnsupportedTimestamp`] (past 47 bits), before any bus access.
    /// - [`Error::Timeout`] if no transport buffer frees in time: a stream not yet started
    ///   (or held by its sync) fills them all and stops taking packets.
    /// - [`Error::Stream`] if the transport fails; any bus failure. A transport failure
    ///   after it lent a buffer leaves that buffer's place in the ring unknown: stop the
    ///   group (or close the device) and create it again rather than sending on.
    pub fn send<S: HostSample>(
        &mut self,
        channels: &[&[S]],
        time: Option<StreamTime>,
        timeout: Duration,
    ) -> Result<(), Error> {
        self.board().send(channels, time, timeout).map(|_| ())
    }

    /// [`Device::send`], returning what the TX core reported with the buffer (libusdr's
    /// `usdr_dms_send_stat`), or `None` when the transport had no report for it: over
    /// `PCIe`, the first 32 buffers, which the driver hands out before any completes.
    ///
    /// # Errors
    ///
    /// As [`Device::send`].
    pub fn send_with_stat<S: HostSample>(
        &mut self,
        channels: &[&[S]],
        time: Option<StreamTime>,
        timeout: Duration,
    ) -> Result<Option<TxTelemetry>, Error> {
        self.board().send(channels, time, timeout)
    }

    /// Mutes the TX core's lanes as `mute` says (libusdr's stream option `"mute"`). The
    /// stream keeps the setting and applies it again when it starts.
    ///
    /// # Errors
    ///
    /// [`Error::NotStreaming`] with no TX stream; any bus failure.
    pub fn set_tx_mute(&mut self, mute: TxMute) -> Result<(), Error> {
        self.board().set_tx_mute(mute)
    }

    /// Stops and destroys the stream group, powering its chains down. The frequency,
    /// bandwidth and sample rate stay set for the next group.
    ///
    /// # Errors
    ///
    /// [`Error::NotStreaming`] with no group; otherwise the first failure, after every step
    /// is attempted.
    pub fn stop_streams(&mut self) -> Result<(), Error> {
        self.board().stop_streams()
    }

    /// Creates and starts a `ci16` channel-0 RX stream, unsynchronised, with
    /// `samples_per_packet` samples in each packet [`Device::receive`] delivers: the FFI
    /// `usdr` crate's flow (create, start, then sync `"none"`), otherwise as
    /// [`Device::create_streams`] then [`Device::start_streams`].
    ///
    /// # Errors
    ///
    /// As [`Device::create_streams`] and [`Device::start_streams`].
    pub fn start_rx_stream(&mut self, samples_per_packet: u32) -> Result<(), Error> {
        self.board().start_rx_stream(samples_per_packet)
    }

    /// Throttles the running RX stream's bursts (`Some`) or delivers them all (`None`),
    /// as libusdr's `/dm/sdr/0/rfe/throttle` does.
    ///
    /// # Errors
    ///
    /// [`Error::NotStreaming`] with no stream; any bus failure.
    pub fn set_rx_throttle(&mut self, throttle: Option<RxThrottle>) -> Result<(), Error> {
        self.board().set_rx_throttle(throttle)
    }

    /// Tells the RX DMA engine the host is ready for blocks (libusdr's
    /// `usdr_dms_set_ready`). [`Device::receive_into`] does this itself before every call
    /// until the first packet arrives; this lets a caller do it earlier, say right after
    /// [`Device::start_streams`].
    ///
    /// # Errors
    ///
    /// [`Error::NotStreaming`] with no RX stream; any bus failure.
    pub fn set_rx_ready(&mut self) -> Result<(), Error> {
        self.board().set_rx_ready()
    }

    /// Samples per packet of the RX stream, or `None` with no stream.
    #[must_use]
    pub fn rx_packet_samples(&self) -> Option<u32> {
        self.board.as_ref().and_then(Board::rx_packet_samples)
    }

    /// Receives one packet of `ci16` IQ samples into the front of `samples`, waiting up to
    /// `timeout`: [`Device::receive_into`] for the one channel.
    ///
    /// # Errors
    ///
    /// As [`Device::receive_into`].
    pub fn receive(
        &mut self,
        samples: &mut [Complex<i16>],
        timeout: Duration,
    ) -> Result<RxPacket, Error> {
        self.board().receive(samples, timeout)
    }

    /// Stops and destroys the RX stream: [`Device::stop_streams`].
    ///
    /// # Errors
    ///
    /// As [`Device::stop_streams`].
    pub fn stop_rx_stream(&mut self) -> Result<(), Error> {
        self.board().stop_streams()
    }

    /// Fixes the RX filter bandwidth in Hz, so later sample-rate and frequency changes keep
    /// it; 0 returns the filter to following the sample rate.
    ///
    /// Like libusdr, setting 0 writes the filter's narrowest setting until the next rate or
    /// frequency change.
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub fn set_rx_bandwidth(&mut self, hz: u32) -> Result<(), Error> {
        self.board().set_rx_bandwidth(hz)
    }

    /// Sets one RX gain stage, or the whole chain with [`RxGain::Auto`]; out-of-range
    /// figures are clamped, as in libusdr. The gain holds across frequency and sample-rate
    /// changes, and RXVGA2's stages survive the DC calibration that starts a stream.
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub fn set_rx_gain(&mut self, gain: RxGain) -> Result<(), Error> {
        self.board().set_rx_gain(gain)
    }

    /// Fixes the RX input, optionally with the TX mixer's output looped back into it, or
    /// with [`RxPath::Auto`] hands the choice back to the frequency. A fixed input holds
    /// across frequency changes.
    ///
    /// Selecting [`RxPath::Auto`] applies the band for the current frequency at once; before
    /// any frequency is set that is the low band, as in libusdr. Entering or leaving the
    /// low band gates the mixer LO as tuning does (Si5332 output 3 alone), but, as in
    /// libusdr, does not retune the PLL: its LO stays off by the mixer LO until the next
    /// [`Device::set_rx_frequency`] (or, in the low band, sample-rate change).
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub fn set_rx_path(&mut self, path: RxPath) -> Result<(), Error> {
        self.board().set_rx_path(path)
    }

    /// Trims the RX mixer's I and Q DC offsets in the LMS6002D (libusdr's `rx/dccorr`).
    ///
    /// As in libusdr, starting a stream calibrates DC and leaves the I trim at 0; the Q
    /// trim stays.
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub fn set_rx_dc_trim(&mut self, i: RxTrim, q: RxTrim) -> Result<(), Error> {
        self.board().set_rx_dc_trim(i, q)
    }

    /// Switches the FPGA RX chain's automatic DC-offset correction (libusdr's
    /// `rx/dccorrmode`, which keeps bit 0 of its value). It is on from the first sample
    /// rate, and every sample-rate change turns it back on, as in libusdr.
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub fn set_rx_auto_dc_correction(&mut self, enabled: bool) -> Result<(), Error> {
        self.board().set_rx_auto_dc_correction(enabled)
    }

    /// Trims the RX mixer's I and Q second-order intermodulation (IP2) in the LMS6002D
    /// (libusdr's `rx/ip2corr`).
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub fn set_rx_ip2_trim(&mut self, i: RxTrim, q: RxTrim) -> Result<(), Error> {
        self.board().set_rx_ip2_trim(i, q)
    }

    /// Sets the FPGA RX chain's IQ gain and phase imbalance correction (libusdr's
    /// `rx/phgaincorr`). It holds for later streams.
    ///
    /// # Errors
    ///
    /// Any bus failure; the correction is still kept for the next stream, as in libusdr.
    pub fn set_rx_iq_correction(&mut self, correction: RxIqCorrection) -> Result<(), Error> {
        self.board().set_rx_iq_correction(correction)
    }

    /// Tunes the transmitter to `hz`, powering it on the first call (libusdr's
    /// `/dm/sdr/0/tx/frequency`, every channel).
    ///
    /// The output follows the frequency, PA1 below 2.8 GHz and PA2 from there, unless
    /// [`Device::set_tx_path`] fixed it. If the PLL cannot lock at an LO below 250 MHz, the
    /// transmitter tunes the lowest LO that does and the TX NCOs cover the rest, which needs
    /// a sample rate with room for the offset, as for RX. The TX filter follows the sample
    /// rate unless [`Device::set_tx_bandwidth`] fixed it. Set a sample rate first: before
    /// one, the NCO fallback fails and the filter is left alone (libusdr divides by zero).
    ///
    /// # Errors
    ///
    /// - [`Error::PllUnlocked`] if the TX PLL cannot lock and no NCO fallback applies.
    /// - [`Error::UnsupportedFrequency`] below the PLL's 170 MHz floor, after the
    ///   transmitter is powered and its output selected, as in libusdr.
    /// - [`Error::PllFault`] if the LMS6002D reads back impossible PLL values.
    /// - Any bus failure.
    pub fn set_tx_frequency(&mut self, hz: u32) -> Result<(), Error> {
        self.board().set_tx_frequency(hz)
    }

    /// Runs libusdr's LO-leakage and IQ-imbalance calibrations (`/dm/sdr/0/calibrate`):
    /// with the transmitter looped into the receiver, it searches the RX IQ correction,
    /// the TX DC offset and the TX IQ correction, as `calibration` asks, and leaves what it
    /// found applied, for later streams too. A calibration needs its chain tuned: RX IQ an
    /// RX frequency, the TX ones a TX frequency; one without is skipped (`None` in the
    /// report). Set a sample rate first.
    ///
    /// Afterwards the tuning, RX input, RXVGA1 gain, generator, NCOs and filters return
    /// to the caller's settings, and TXVGA1's gain does after the TX IQ search; a chain
    /// with no frequency is powered down. As in libusdr, some changes stay: the RX IQ
    /// search can leave TXVGA1 raised to level its tone; the analog LO stage leaves
    /// TXVGA1's DC shift (which the report does not give); a chain that was tuned but
    /// powered down (as TX is after a stream group with TX) is powered and left on; and a
    /// receiver the TX calibrations had to wake keeps the DC calibration they ran on it
    /// at 320 MHz. To reapply the results after reopening, pass the report's offsets to
    /// [`Device::set_tx_dc_offset`] and its corrections to
    /// [`Device::set_rx_iq_imbalance`] and [`Device::set_tx_iq_imbalance`].
    ///
    /// # Errors
    ///
    /// - [`Error::AlreadyStreaming`] while a stream group exists, before any bus access:
    ///   the calibration takes over the generator, NCOs and both PLLs.
    /// - [`Error::NoSampleRate`] before any sample rate, before any bus access (libusdr
    ///   divides by zero).
    /// - [`Error::PllUnlocked`], [`Error::UnsupportedFrequency`], [`Error::PllFault`] if a
    ///   PLL cannot reach a measuring frequency.
    /// - [`Error::Timeout`] if the gateware's DC estimator never finishes a measurement.
    /// - Any bus failure. A failure stops the calibration where it is, without restoring,
    ///   as in libusdr.
    pub fn calibrate(&mut self, calibration: Calibration) -> Result<CalibrationReport, Error> {
        self.board().calibrate(calibration)
    }

    /// Sets the FPGA RX chain's IQ correction in the terms [`Device::calibrate`] reports,
    /// such as a saved calibration result; it holds for later streams. libusdr has no
    /// control for these terms: its calibration writes them through `usdr_rxupdate_cal`.
    ///
    /// # Errors
    ///
    /// Any bus failure; the correction is still kept for the next stream.
    pub fn set_rx_iq_imbalance(&mut self, imbalance: IqImbalance) -> Result<(), Error> {
        self.board().set_rx_iq_imbalance(imbalance)
    }

    /// Sets the FPGA TX chain's IQ correction in the terms [`Device::calibrate`] reports,
    /// as [`Device::set_rx_iq_imbalance`] does for RX (`usdr_txupdate_cal`).
    ///
    /// # Errors
    ///
    /// Any bus failure; the correction is still kept for the next TX stream.
    pub fn set_tx_iq_imbalance(&mut self, imbalance: IqImbalance) -> Result<(), Error> {
        self.board().set_tx_iq_imbalance(imbalance)
    }

    /// Fixes the TX output, or with [`TxPath::Auto`] hands the choice back to the
    /// frequency, applying the band for the current TX frequency at once (before any, PA1).
    /// A fixed output holds across frequency changes; neither retunes.
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub fn set_tx_path(&mut self, path: TxPath) -> Result<(), Error> {
        self.board().set_tx_path(path)
    }

    /// Fixes the TX filter bandwidth in Hz, so later TX frequency changes keep it; 0 returns
    /// the filter to following the sample rate. Above 47 MHz the filter is bypassed.
    ///
    /// Like libusdr, setting 0 writes the filter's narrowest setting until the next TX
    /// frequency change; sample-rate changes leave the TX filter alone.
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub fn set_tx_bandwidth(&mut self, hz: u32) -> Result<(), Error> {
        self.board().set_tx_bandwidth(hz)
    }

    /// Sets one TX gain stage; out-of-range figures are clamped, as in libusdr.
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub fn set_tx_gain(&mut self, gain: TxGain) -> Result<(), Error> {
        self.board().set_tx_gain(gain)
    }

    /// Sets the FPGA TX chain's I and Q DC offsets, which cancel LO leakage (libusdr's
    /// `tx/dccorr`).
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub fn set_tx_dc_offset(&mut self, i: TxDcOffset, q: TxDcOffset) -> Result<(), Error> {
        self.board().set_tx_dc_offset(i, q)
    }

    /// Sets the FPGA TX chain's IQ gain and phase imbalance correction (libusdr's
    /// `tx/phgaincorr`).
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub fn set_tx_iq_correction(&mut self, correction: TxIqCorrection) -> Result<(), Error> {
        self.board().set_tx_iq_correction(correction)
    }

    /// Replaces the TX chain's samples with the constant I/Q pair `constant` on both
    /// channels, or with `None` passes the host's samples again (libusdr's
    /// `tfe/generator/const`). With the TX NCOs offset, the constant becomes a tone: the
    /// test signal of libusdr's LO leakage calibration.
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub fn set_tx_constant(&mut self, constant: Option<Complex<i16>>) -> Result<(), Error> {
        self.board().set_tx_constant(constant)
    }

    /// Waits until the board is below the thermal policy's resume limit, reading every
    /// 5 s and passing each reading to `on_reading`.
    ///
    /// # Errors
    ///
    /// [`Error::Overheated`], with the resume limit, if the board is still too warm after
    /// `timeout`; any bus failure.
    pub fn wait_for_temperature(
        &mut self,
        timeout: Duration,
        on_reading: impl FnMut(f32),
    ) -> Result<(), Error> {
        let mut cool_down = CoolDown {
            timeout,
            on_reading: Box::new(on_reading),
        };
        let limits = self.thermal.limits();
        cool_down.wait(self.board(), limits)
    }

    /// Powers the board down.
    ///
    /// # Errors
    ///
    /// The first bus failure; every power-down step is still attempted.
    pub fn close(mut self) -> Result<(), Error> {
        self.power_down()
    }

    /// The powered board.
    fn board(&mut self) -> &mut Board {
        self.board
            .as_mut()
            .expect("invariant: a Device owns its board until close or drop")
    }

    /// Stops any stream group, then powers the board down, if it is still up. Every step is
    /// attempted; the first failure is returned.
    fn power_down(&mut self) -> Result<(), Error> {
        let Some(mut board) = self.board.take() else {
            return Ok(());
        };
        let stopped = if board.has_streams() {
            board.stop_streams()
        } else {
            Ok(())
        };
        stopped.and(board.power_down())
    }
}

impl<'a> DeviceBuilder<'a> {
    /// Takes the clocks' reference from `reference` instead of [`ReferenceClock::Board`]
    /// (libusdr's `extclk` and `extref` options).
    #[must_use]
    pub fn reference_clock(mut self, reference: ReferenceClock) -> Self {
        self.reference = reference;
        self
    }

    /// Looks for the front end `selection` names after power-up instead of probing every
    /// board ([`FrontEndSelection::Auto`], libusdr's default; libusdr's `fe` option).
    #[must_use]
    pub fn front_end(mut self, selection: FrontEndSelection) -> Self {
        self.front_end = selection;
        self
    }

    /// Enforces `policy` instead of [`ThermalPolicy::WithinSpec`].
    #[must_use]
    pub fn thermal(mut self, policy: ThermalPolicy) -> Self {
        self.thermal = policy;
        self
    }

    /// If the board is too hot to start, waits up to `timeout` for it to cool below the
    /// policy's resume limit, passing each reading (every 5 s) to `on_reading`.
    #[must_use]
    pub fn wait_for_temperature(
        mut self,
        timeout: Duration,
        on_reading: impl FnMut(f32) + 'a,
    ) -> Self {
        self.cool_down = Some(CoolDown {
            timeout,
            on_reading: Box::new(on_reading),
        });
        self
    }

    /// Identifies the board, confirms its temperature sensor, applies the thermal policy
    /// before anything is powered, powers the board up, then probes for a front end.
    ///
    /// # Errors
    ///
    /// - [`Error::UnsupportedReferenceClock`] for an unusable external reference, before
    ///   any bus access. On revision 3 with an external reference the oscillator stays off;
    ///   like libusdr, a Si5332 that then finds no input clock does not fail power-up.
    /// - Whatever a selected front end's bring-up fails with ([`Error::FrontEnd`],
    ///   [`Error::ClockUnlocked`], a bus error, ...; open with [`FrontEndSelection::None`]
    ///   to skip the probe). The board is then powered down again, where libusdr leaves it
    ///   powered.
    /// - [`Error::Overheated`] if the board is too hot: with the start limit when failing
    ///   at once, with the resume limit after a configured wait times out.
    /// - [`Error::ChipId`] if no TMP114 answers, so temperature cannot be trusted.
    /// - Any bus failure, an unsupported board revision, or another chip that does not
    ///   identify itself as expected.
    pub fn open(self) -> Result<Device, Error> {
        let Self {
            bus,
            thermal,
            cool_down,
            reference,
            front_end,
        } = self;
        reference.check()?;
        let mut identified = Board::identify(bus)?;
        identified.check_temperature_sensor()?;
        let limits = thermal.limits();
        let celsius = identified.temperature()?;
        if celsius >= limits.start() {
            let Some(mut cool_down) = cool_down else {
                return Err(Error::Overheated {
                    celsius,
                    limit: limits.start(),
                });
            };
            warn!(
                celsius,
                limit = limits.start(),
                "too hot to start; waiting to cool"
            );
            cool_down.wait(&mut identified, limits)?;
        }
        let mut board = identified.power_up(reference)?;
        if let Err(error) = board.probe_front_end(&front_end) {
            warn!(%error, "front-end probe failed; powering the board down");
            // Best effort: the probe's failure is the one to report.
            let _ = board.power_down();
            return Err(error);
        }
        Ok(Device {
            board: Some(board),
            thermal,
        })
    }
}

impl fmt::Debug for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Device")
            .field("powered", &self.board.is_some())
            .field("thermal", &self.thermal)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for DeviceBuilder<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeviceBuilder")
            .field("thermal", &self.thermal)
            .field(
                "cool_down",
                &self.cool_down.as_ref().map(|cool_down| cool_down.timeout),
            )
            .field("front_end", &self.front_end)
            .finish_non_exhaustive()
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        // Errors are returned only by `close`; here they can only be logged.
        if let Err(error) = self.power_down() {
            warn!(%error, "power-down on drop failed");
        }
    }
}
