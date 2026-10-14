//! Setting the sample rate and the fixed bandwidths: the portable [`SampleRate`]
//! (`usdr_set_samplerate_ex` in `device/m2_lm6_1/usdr_ctrl.c`) and
//! `usdr_rfic_bb_set_badwidth`, run over the board's bus.

use deps::tracing::debug;

use super::Board;
use crate::error::Error;
use crate::portable::radio::{RateChains, SampleRate};

impl Board {
    /// Sets the sample rate for `chains`, in samples per second ([`SampleRate`]).
    ///
    /// # Errors
    ///
    /// [`Error::AlreadyStreaming`] while a stream group exists and
    /// [`Error::UnsupportedSampleRate`] for a rate the board cannot run, before any bus
    /// access; then the rate change's failures.
    pub(crate) fn set_sample_rate(&mut self, rate: u32, chains: RateChains) -> Result<(), Error> {
        // The block size and decimation are fixed while a stream runs; libusdr's callers only
        // set the rate before creating one.
        if self.streams.is_some() {
            return Err(Error::AlreadyStreaming);
        }
        let change =
            SampleRate::new(self.radio, rate, chains).ok_or(Error::UnsupportedSampleRate(rate))?;
        let (factor, adc_rate_hz) = (change.factor(), change.adc_rate_hz());
        self.run_radio(change)?;
        debug!(
            rate,
            ?chains,
            ?factor,
            adc_rate_hz,
            mixer_lo_hz = self.radio.mixer_lo_hz,
            vio_boosted = self.radio.vio_boosted,
            "sample rate set"
        );
        Ok(())
    }

    /// Fixes the RX filter bandwidth in Hz, or with 0 returns it to following the sample
    /// rate (`usdr_rfic_bb_set_badwidth`). Either way the filter is written now: for 0,
    /// like libusdr, that is its narrowest setting, until the next rate or frequency
    /// change.
    pub(crate) fn set_rx_bandwidth(&mut self, hz: u32) -> Result<(), Error> {
        self.radio.rx.fixed_bandwidth_hz = (hz != 0).then_some(hz);
        let script = self.radio.lms.set_rx_bandwidth(hz);
        self.run_script(script)
    }

    /// Fixes the TX filter bandwidth in Hz, or with 0 returns it to following the sample
    /// rate, as [`Self::set_rx_bandwidth`] does for RX (`usdr_rfic_bb_set_badwidth` for
    /// TX). The rate follows on a TX tune and a rate for TX: libusdr's RX-only rate change
    /// leaves the TX filter alone.
    pub(crate) fn set_tx_bandwidth(&mut self, hz: u32) -> Result<(), Error> {
        self.radio.tx.fixed_bandwidth_hz = (hz != 0).then_some(hz);
        let script = self.radio.lms.set_tx_bandwidth(hz);
        self.run_script(script)
    }
}
