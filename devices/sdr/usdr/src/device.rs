//! The radio as a caller sees it.

use std::fmt;
use std::time::Duration;

use num_complex::Complex;
use rsl_deps::tracing::warn;

use crate::board::{Board, RxGain, RxIqCorrection, RxPacket, RxPath, RxThrottle, RxTrim};
use crate::error::Error;
use crate::identity::Identity;
use crate::lowlevel::Bus;
use crate::reference::ReferenceClock;
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
    /// - [`Error::AlreadyStreaming`] while an RX stream runs.
    /// - [`Error::ClockInputMissing`] if the Si5332 loses its reference while switching.
    /// - [`Error::PllUnlocked`], [`Error::PllFault`] or [`Error::UnsupportedFrequency`] if
    ///   the receiver is tuned below 230 MHz and retuning it for the new mixer LO fails.
    /// - Any bus failure.
    pub fn set_rx_sample_rate(&mut self, samples_per_second: u32) -> Result<(), Error> {
        self.board().set_rx_sample_rate(samples_per_second)
    }

    /// Tunes the receiver to `hz`.
    ///
    /// Below 230 MHz the board mixer lifts the signal into the LMS6002D's range, so the
    /// PLL's LO is about 300 MHz higher. If the PLL cannot lock at an LO below 250 MHz (in
    /// practice a request from 230 to 250 MHz), the receiver tunes the lowest LO that does
    /// and the NCO covers the rest, which needs a sample rate with room for the offset.
    ///
    /// # Errors
    ///
    /// - [`Error::PllUnlocked`] if the RX PLL cannot lock and no NCO fallback applies.
    /// - [`Error::UnsupportedFrequency`] for an LO below the PLL's 170 MHz floor, possible
    ///   only under 230 MHz before a sample rate sets the mixer LO.
    /// - [`Error::PllFault`] if the LMS6002D reads back impossible PLL values.
    /// - Any bus failure.
    pub fn set_rx_frequency(&mut self, hz: u32) -> Result<(), Error> {
        self.board().set_rx_frequency(hz)
    }

    /// Creates the RX stream and starts it, with `samples_per_packet` samples in each packet
    /// [`Device::receive`] delivers. Frequency and bandwidth can change while it runs; the
    /// sample rate cannot.
    ///
    /// Creating the stream calibrates the receiver's DC offsets. If no frequency was set,
    /// the receiver tunes to 320 MHz first, as libusdr does.
    ///
    /// # Errors
    ///
    /// - [`Error::AlreadyStreaming`] if a stream is running.
    /// - [`Error::UnsupportedPacketSize`] for a packet the stream engine cannot split into
    ///   bursts.
    /// - [`Error::Stream`] if the transport cannot set up its buffers, or does not stream.
    /// - [`Error::DmaEngineFault`] if the FPGA's DMA engine does not respond.
    /// - Tuning and calibration errors, as [`Device::set_rx_frequency`]; any bus failure.
    ///
    /// As in libusdr, a failure after the transport set up its buffers leaves the stream in
    /// place: call [`Device::stop_rx_stream`] before starting again.
    /// For USB, an ambiguous control transaction permanently invalidates the bus. Stopping
    /// still attempts to drain its RX buffers, but close/drop and reopen the device rather
    /// than retrying start on that handle. Power-down writes may fail on the invalid bus.
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

    /// Samples per packet of the running RX stream, or `None` with no stream.
    #[must_use]
    pub fn rx_packet_samples(&self) -> Option<u32> {
        self.board.as_ref().and_then(Board::rx_packet_samples)
    }

    /// Receives one packet of IQ samples into the front of `samples`, waiting up to
    /// `timeout`; the rest of `samples` is untouched.
    ///
    /// # Errors
    ///
    /// - [`Error::Timeout`] if no packet arrives in time.
    /// - [`Error::BufferTooSmall`] if `samples` holds less than one packet.
    /// - [`Error::NotStreaming`] with no stream.
    /// - [`Error::Stream`] if the transport fails; any bus failure.
    pub fn receive(
        &mut self,
        samples: &mut [Complex<i16>],
        timeout: Duration,
    ) -> Result<RxPacket, Error> {
        self.board().receive(samples, timeout)
    }

    /// Stops and destroys the RX stream, powering the receiver down. The frequency,
    /// bandwidth and sample rate stay set for the next stream.
    ///
    /// # Errors
    ///
    /// [`Error::NotStreaming`] with no stream; otherwise the first failure, after every step
    /// is attempted.
    pub fn stop_rx_stream(&mut self) -> Result<(), Error> {
        self.board().stop_rx_stream()
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
    /// low band gates the mixer LO as tuning does (Si5332 output 3 alone).
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

    /// Stops any RX stream, then powers the board down, if it is still up. Every step is
    /// attempted; the first failure is returned.
    fn power_down(&mut self) -> Result<(), Error> {
        let Some(mut board) = self.board.take() else {
            return Ok(());
        };
        let stopped = if board.rx_packet_samples().is_some() {
            board.stop_rx_stream()
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
    /// before anything is powered, then powers the board up.
    ///
    /// # Errors
    ///
    /// - [`Error::UnsupportedReferenceClock`] for an unusable external reference, before
    ///   any bus access. On revision 3 with an external reference the oscillator stays off;
    ///   like libusdr, a Si5332 that then finds no input clock does not fail power-up.
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
        let board = identified.power_up(reference)?;
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
