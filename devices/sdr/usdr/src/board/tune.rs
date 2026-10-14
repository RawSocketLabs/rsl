//! Tuning, gains and paths: the portable machines (`portable::radio`'s [`Tune`],
//! [`SetRxPath`], the gain and path scripts) run over the board's bus, as
//! `/dm/sdr/0/rx/freqency`, `/dm/sdr/0/tx/frequency`, their per-channel and baseband
//! forms, the gain and the path controls reach `usdr_ctrl.c`.

use deps::tracing::debug;

use super::Board;
use crate::chips::register::run_sequence;
use crate::error::Error;
use crate::portable::lms6002d::{CapacitorWindow, RxDcCalibration};
use crate::portable::radio::{
    Chain, ChainPower, NcoChannels, RxGain, RxPath, SetRxPath, Tune, TxGain, TxPath,
};

impl Board {
    /// Tunes the receiver to `hz`, both NCO channels.
    pub(crate) fn set_rx_frequency(&mut self, hz: u32) -> Result<(), Error> {
        self.set_rx_channel_frequency(NcoChannels::Both, hz)
    }

    /// Tunes the transmitter to `hz`, both NCO channels.
    pub(crate) fn set_tx_frequency(&mut self, hz: u32) -> Result<(), Error> {
        self.set_tx_channel_frequency(NcoChannels::Both, hz)
    }

    /// Tunes `channels` of the receiver to `hz` and the PLL to the average of the channels'
    /// LOs (`usdr_rfic_fe_set_freq` with `FE_FREQ_LO_RX` and a channel mask).
    pub(crate) fn set_rx_channel_frequency(
        &mut self,
        channels: NcoChannels,
        hz: u32,
    ) -> Result<(), Error> {
        self.tune(Tune::channel_frequency(self.radio, Chain::Rx, channels, hz))
    }

    /// Tunes `channels` of the transmitter, as [`Self::set_rx_channel_frequency`] does the
    /// receiver's (`FE_FREQ_LO_TX`).
    pub(crate) fn set_tx_channel_frequency(
        &mut self,
        channels: NcoChannels,
        hz: u32,
    ) -> Result<(), Error> {
        self.tune(Tune::channel_frequency(self.radio, Chain::Tx, channels, hz))
    }

    /// Sets the RX baseband offset of `channels`, then tunes the receiver again to its LO
    /// (`usdr_rfic_fe_set_freq` with `FE_FREQ_BB_RX`).
    pub(crate) fn set_rx_baseband_offset(
        &mut self,
        channels: NcoChannels,
        hz: i32,
    ) -> Result<(), Error> {
        self.tune(Tune::baseband_offset(self.radio, Chain::Rx, channels, hz))
    }

    /// Sets the TX baseband offset of `channels`, then tunes the transmitter again to its
    /// LO (`FE_FREQ_BB_TX`).
    pub(crate) fn set_tx_baseband_offset(
        &mut self,
        channels: NcoChannels,
        hz: i32,
    ) -> Result<(), Error> {
        self.tune(Tune::baseband_offset(self.radio, Chain::Tx, channels, hz))
    }

    /// Runs `tune`.
    fn tune(&mut self, tune: Tune) -> Result<(), Error> {
        self.run_radio(tune)?;
        debug!(
            rx_hz = self.radio.rx.lo_hz,
            rx_nco_offset_hz = self.radio.rx.lo_offset_hz,
            tx_hz = self.radio.tx.lo_hz,
            tx_nco_offset_hz = self.radio.tx.lo_offset_hz,
            "tuned"
        );
        Ok(())
    }

    /// Tunes `chain`'s PLL to `lo` (`lms6002d_tune_pll`).
    pub(super) fn tune_pll(&mut self, chain: Chain, lo: u32) -> Result<CapacitorWindow, Error> {
        let mut tune = self.radio.tune_pll(chain, lo);
        let ran = run_sequence(self.bus.as_mut(), &mut tune);
        self.radio.lms = *tune.chip();
        ran?;
        Ok(tune
            .outcome()
            .expect("invariant: a tuning without a bus failure finishes")?)
    }

    /// Sets an RX gain (`usdr_rfic_set_gain` for the RX stages).
    pub(crate) fn set_rx_gain(&mut self, gain: RxGain) -> Result<(), Error> {
        let script = self.radio.set_rx_gain(gain);
        self.run_script(script)
    }

    /// Sets a TX gain (`usdr_rfic_set_gain` for the TX stages).
    pub(crate) fn set_tx_gain(&mut self, gain: TxGain) -> Result<(), Error> {
        let script = self.radio.set_tx_gain(gain);
        self.run_script(script)
    }

    /// Selects the TX path (`usdr_rfic_fe_set_txlna`).
    pub(crate) fn set_tx_path(&mut self, path: TxPath) -> Result<(), Error> {
        let script = self.radio.set_tx_path(path);
        self.run_script(script)
    }

    /// Selects the RX path (`usdr_rfic_fe_set_rxlna`), then switches the RF loopback.
    pub(crate) fn set_rx_path(&mut self, path: RxPath) -> Result<(), Error> {
        self.run_radio(SetRxPath::new(self.radio, path))
    }

    /// Powers the LMS6002D's receiver and RXVGA2 on or off, if they are not already
    /// (`_usdr_pwr_state` for RX).
    pub(super) fn set_rx_power(&mut self, on: bool) -> Result<(), Error> {
        self.run_radio(ChainPower::new(self.radio, Chain::Rx, on))
    }

    /// Powers the LMS6002D's transmitter on or off, if it is not already
    /// (`_usdr_pwr_state` for TX).
    pub(super) fn set_tx_power(&mut self, on: bool) -> Result<(), Error> {
        self.run_radio(ChainPower::new(self.radio, Chain::Tx, on))
    }

    /// Tunes the RX PLL to `lo` (`lms6002d_tune_pll`).
    pub(super) fn tune_rx_pll(&mut self, lo: u32) -> Result<CapacitorWindow, Error> {
        self.tune_pll(Chain::Rx, lo)
    }

    /// Tunes the TX PLL to `lo` (`lms6002d_tune_pll`).
    pub(super) fn tune_tx_pll(&mut self, lo: u32) -> Result<CapacitorWindow, Error> {
        self.tune_pll(Chain::Tx, lo)
    }

    /// Points `chain`'s NCOs at its offsets (`usdr_restore_nco`).
    pub(super) fn restore_ncos(&mut self, chain: Chain) -> Result<(), Error> {
        let script = self.radio.restore_ncos(chain);
        self.run_script(script)
    }

    /// Points both RX NCOs at their offsets (`usdr_restore_nco` for RX).
    pub(super) fn restore_rx_ncos(&mut self) -> Result<(), Error> {
        self.restore_ncos(Chain::Rx)
    }

    /// Sets the RX filter for the rate and offsets, unless fixed or no rate is set
    /// (`_usdr_update_bandwidth` for RX).
    pub(super) fn update_rx_bandwidth(&mut self) -> Result<(), Error> {
        self.radio
            .update_bandwidth(Chain::Rx)
            .map_or(Ok(()), |script| self.run_script(script))
    }

    /// Sets the TX filter likewise (`_usdr_update_bandwidth` for TX).
    pub(super) fn update_tx_bandwidth(&mut self) -> Result<(), Error> {
        self.radio
            .update_bandwidth(Chain::Tx)
            .map_or(Ok(()), |script| self.run_script(script))
    }

    /// libusdr's RX DC-offset calibration ([`RxDcCalibration`]), restoring RXVGA2 to the
    /// caller's steps.
    pub(super) fn calibrate_rx_dc(&mut self) -> Result<(), Error> {
        let radio = self.radio;
        let vga2 = (radio.rx.vga2a, radio.rx.vga2b);
        let mut calibration = RxDcCalibration::new(radio.lms, radio.reference_hz, vga2);
        let ran = run_sequence(self.bus.as_mut(), &mut calibration);
        self.radio.lms = *calibration.chip();
        calibration.fault().map_or(ran, |fault| Err(fault.into()))
    }
}
