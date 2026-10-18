//! Tuning, gains and paths: the portable machines (`portable::radio`'s [`Tune`],
//! [`SetRxPath`], the gain and path scripts) run over the board's bus, as
//! `/dm/sdr/0/rx/freqency`, `/dm/sdr/0/tx/frequency`, their per-channel and baseband
//! forms, the gain and the path controls reach `usdr_ctrl.c`.

use deps::tracing::debug;

use super::Board;
use crate::error::Error;
use crate::portable::radio::{Chain, NcoChannels, RxGain, RxPath, SetRxPath, Tune, TxGain, TxPath};

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
}
