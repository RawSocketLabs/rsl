//! Tuning: `usdr_rfic_fe_set_freq` for the RX and TX LOs in `device/m2_lm6_1/usdr_ctrl.c`,
//! as `/dm/sdr/0/rx/freqency` and `/dm/sdr/0/tx/frequency` reach it (every channel at
//! once).
//!
//! The RX frequency picks a band: below 230 MHz the board mixer adds the Si5332's mixer LO
//! (output 3) to lift the signal into LNA3's range, so the LMS6002D tunes that much higher;
//! up to 2.8 GHz LNA1 takes it directly, and LNA2 above, unless the caller fixed the path
//! (`board/path.rs`). The TX frequency picks PA1 up to 2.8 GHz and PA2 above, unless fixed.
//! The chain's PLL then tunes. Below 250 MHz, if it cannot lock, libusdr finds the lowest LO
//! that does and makes up the difference with the chain's NCOs, if the sample rate leaves
//! room.

use std::time::Duration;

use rsl_deps::tracing::debug;

use super::Board;
use super::baseband::NcoChannels;
use crate::chips::lms6002d::{CapacitorWindow, Lna, PowerAmp};
use crate::error::Error;
use crate::fpga::{Decimation, Gpo, Nco, Phy};

/// A signal chain: which synthesizer, NCOs and filter a tune drives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Chain {
    /// The receive chain.
    Rx,

    /// The transmit chain.
    Tx,
}

impl Chain {
    /// The chain's FPGA configuration port, where its NCOs are.
    fn phy(self) -> Phy {
        match self {
            Self::Rx => Phy::Rx,
            Self::Tx => Phy::Tx,
        }
    }
}

/// Where the RX path runs (libusdr's `cfg_auto_rx` on boards with the mixer, which every
/// supported revision has).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RxBand {
    /// Below 230 MHz: the board mixer into LNA3 (`LNAL`).
    Mixer,

    /// 230 MHz to 2.8 GHz: LNA1, the wideband input (`LNAW`).
    Wide,

    /// 2.8 GHz and up: LNA2 (`LNAH`).
    High,
}

impl RxBand {
    /// The mixer band ends here, in Hz.
    const MIXER_BELOW_HZ: u32 = 230_000_000;

    /// The wide band ends here, in Hz.
    const WIDE_BELOW_HZ: u32 = 2_800_000_000;

    /// The band for a frequency (`get_antenna_cfg_by_freq`).
    pub(super) fn for_frequency(hz: u32) -> Self {
        if hz < Self::MIXER_BELOW_HZ {
            Self::Mixer
        } else if hz < Self::WIDE_BELOW_HZ {
            Self::Wide
        } else {
            Self::High
        }
    }

    /// The LMS6002D input.
    fn lna(self) -> Lna {
        match self {
            Self::Mixer => Lna::Lna3,
            Self::Wide => Lna::Lna1,
            Self::High => Lna::Lna2,
        }
    }

    /// The RX antenna switch setting ([`Gpo::RxSwitch`]).
    fn rx_switch(self) -> u8 {
        match self {
            Self::Wide => 0,
            Self::Mixer | Self::High => 1,
        }
    }

    /// The board mixer is in the path.
    fn mixer(self) -> bool {
        self == Self::Mixer
    }
}

/// Where the TX path runs (libusdr's `cfg_auto_tx`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TxBand {
    /// Below 2.8 GHz: PA1 (`W`, also `B1`).
    Wide,

    /// 2.8 GHz and up: PA2 (`H`, also `B2`).
    High,
}

impl TxBand {
    /// The wide band ends here, in Hz.
    const WIDE_BELOW_HZ: u32 = 2_800_000_000;

    /// The band for a frequency (`get_antenna_cfg_by_freq`).
    pub(super) fn for_frequency(hz: u32) -> Self {
        if hz < Self::WIDE_BELOW_HZ {
            Self::Wide
        } else {
            Self::High
        }
    }

    /// The LMS6002D output amplifier.
    fn amplifier(self) -> PowerAmp {
        match self {
            Self::Wide => PowerAmp::Pa1,
            Self::High => PowerAmp::Pa2,
        }
    }

    /// The TX antenna switch setting ([`Gpo::TxSwitch`]).
    fn tx_switch(self) -> u8 {
        match self {
            Self::Wide => 1,
            Self::High => 0,
        }
    }
}

impl Board {
    /// Below this LO, in Hz, a PLL that cannot lock falls back to the lowest LO that can,
    /// plus an NCO offset (`USDR_LO_LOW_RANGE`).
    const LOW_LO_HZ: u32 = 250_000_000;

    /// The share of the ADC rate an NCO offset plus the signal may occupy.
    const NCO_HEADROOM: f64 = 0.45;

    /// Tunes the receiver to `hz`, both NCO channels.
    pub(crate) fn set_rx_frequency(&mut self, hz: u32) -> Result<(), Error> {
        self.set_rx_channel_frequency(NcoChannels::Both, hz)
    }

    /// Tunes the receiver's PLL to `hz` with the chain's frequency plan: the rest of
    /// `usdr_rfic_fe_set_freq` for RX once the channels' LOs and offsets are recorded.
    ///
    /// The band follows the frequency unless [`Self::set_rx_path`] fixed it.
    ///
    /// Divergences, both where libusdr would divide by zero before any sample rate is set:
    /// the NCO lock fallback fails with [`Error::PllUnlocked`], and the filter is left
    /// alone. A third, reachable only with the mixer band fixed: where `hz` plus the mixer
    /// LO passes `u32::MAX`, libusdr converts the double sum to `unsigned`, which C leaves
    /// undefined (x86 wraps it), and tunes that; this fails with
    /// [`Error::UnsupportedFrequency`] before tuning. On failure, state is left as libusdr
    /// leaves it: the band change as [`Self::select_rx_band`] says, and the LO offset and
    /// minimal LO recorded at the same points.
    pub(super) fn tune_rx(&mut self, hz: u32) -> Result<(), Error> {
        self.rx.lo_hz = hz;
        // `_usdr_signal_event(USDR_RX_LO_CHANGED)`: power the receiver, ignoring a failure,
        // then the band for the frequency, if it is not fixed. libusdr discards the result.
        let _ = self.power_rx();
        if !self.rx.band_fixed {
            let _ = self.select_rx_band(RxBand::for_frequency(hz));
        }

        self.rx.lo_offset_hz = 0;
        let lo = if self.rx.mixer_on {
            hz.checked_add(self.mixer_lo_hz)
                .ok_or(Error::UnsupportedFrequency(hz))?
        } else {
            hz
        };
        self.rx.pll_lo_hz = lo;
        let mut tuned = self.tune_rx_pll(lo);
        if matches!(tuned, Err(Error::PllUnlocked(_))) {
            debug!(
                lo_hz = lo,
                "RX PLL did not lock; retrying once the LDO settles"
            );
            // libusdr: the LDO may not be ready yet.
            self.bus.sleep(Duration::from_millis(5));
            tuned = self.tune_rx_pll(lo);
        }
        if let Err(Error::PllUnlocked(_)) = tuned {
            let mut minimal_lo_hz = self.rx.minimal_lo_hz;
            let offset = self.extend_low_lo(Chain::Rx, lo, &mut minimal_lo_hz);
            self.rx.minimal_lo_hz = minimal_lo_hz;
            self.rx.lo_offset_hz = offset?;
        } else {
            tuned?;
        }

        self.restore_rx_ncos()?;
        self.update_rx_bandwidth()?;
        debug!(
            hz,
            band = ?self.rx.band,
            pll_hz = lo,
            nco_offset_hz = self.rx.lo_offset_hz,
            "RX tuned"
        );
        Ok(())
    }

    /// Routes the RX path for `band` (`_usdr_set_lna_rx`, `usdr_set_rx_port_switch`).
    ///
    /// Error handling follows libusdr's: the band is recorded first, a failed LNA
    /// selection stops before the switches, and the mixer state is recorded even when a
    /// switch write fails, though the Si5332 is then not touched.
    pub(super) fn select_rx_band(&mut self, band: RxBand) -> Result<(), Error> {
        self.rx.band = Some(band);
        let bus = self.bus.as_mut();
        self.lms.select_lna(bus, band.lna())?;
        let switched = Gpo::RxSwitch
            .set(bus, band.rx_switch())
            .and_then(|()| Gpo::RxMixerEnable.set(bus, u8::from(band.mixer())));
        if band.mixer() == self.rx.mixer_on {
            return switched;
        }
        self.rx.mixer_on = band.mixer();
        switched?;
        Self::CLOCK.set_mixer_lo(bus, self.rx.mixer_on)
    }

    /// Powers the LMS6002D's receiver and RXVGA2 if they are off (`_usdr_pwr_state` for
    /// RX).
    pub(super) fn power_rx(&mut self) -> Result<(), Error> {
        self.set_rx_power(true)
    }

    /// Powers the LMS6002D's receiver and RXVGA2 on or off, if they are not already
    /// (`_usdr_pwr_state` for RX). Like libusdr, the new state is recorded once the first
    /// write succeeds, and the 25 ms settling wait follows.
    pub(super) fn set_rx_power(&mut self, on: bool) -> Result<(), Error> {
        if self.rx.powered == on {
            return Ok(());
        }
        let bus = self.bus.as_mut();
        self.lms.set_rx_enabled(bus, on)?;
        self.rx.powered = on;
        if on {
            self.lms.enable_rx_vga2(bus)?;
        } else {
            self.lms.disable_rx_vga2(bus)?;
        }
        bus.sleep(Duration::from_millis(25));
        Ok(())
    }

    /// Tunes the transmitter to `hz`, both NCO channels.
    pub(crate) fn set_tx_frequency(&mut self, hz: u32) -> Result<(), Error> {
        self.set_tx_channel_frequency(NcoChannels::Both, hz)
    }

    /// Tunes the TX chain's PLL to `hz` with the chain's frequency plan
    /// (`usdr_rfic_fe_set_freq` for the TX LO once the channels' LOs and offsets are
    /// recorded).
    ///
    /// The band follows the frequency unless [`Self::set_tx_path`] fixed it. As in libusdr,
    /// the first TX tune that cannot lock, with no RX frequency set, tunes the RX PLL to
    /// the same LO before retrying ("some LMS6002 chips fail to lock TX pll before RX
    /// initialization"), and the low-LO fallback is the receiver's, on the TX PLL and NCOs.
    ///
    /// Divergences, where libusdr would divide by zero before any sample rate is set: the
    /// NCO lock fallback fails with [`Error::PllUnlocked`], and the filter is left alone
    /// (`tests/tx_tuning.rs`). On failure, state is left as libusdr leaves it: the power and
    /// band changes, LO offset and minimal LO recorded at the same points.
    pub(super) fn tune_tx(&mut self, hz: u32) -> Result<(), Error> {
        let first = self.tx.lo_hz == 0;
        self.tx.lo_hz = hz;
        // `_usdr_signal_event(USDR_TX_LO_CHANGED)`: power the transmitter, then the band for
        // the frequency, if it is not fixed. libusdr discards both results.
        let _ = self.power_tx();
        if !self.tx.band_fixed {
            let _ = self.select_tx_band(TxBand::for_frequency(hz));
        }

        self.tx.lo_offset_hz = 0;
        let mut tuned = self.tune_tx_pll(hz);
        if matches!(tuned, Err(Error::PllUnlocked(_))) {
            debug!(lo_hz = hz, "TX PLL did not lock; retrying");
            if first && self.rx.lo_hz == 0 {
                // libusdr discards this tune's result too.
                let _ = self.tune_rx_pll(hz);
            }
            self.bus.sleep(Duration::from_millis(5));
            tuned = self.tune_tx_pll(hz);
        }
        if let Err(Error::PllUnlocked(_)) = tuned {
            let mut minimal_lo_hz = self.tx.minimal_lo_hz;
            let offset = self.extend_low_lo(Chain::Tx, hz, &mut minimal_lo_hz);
            self.tx.minimal_lo_hz = minimal_lo_hz;
            self.tx.lo_offset_hz = offset?;
        } else {
            tuned?;
        }

        self.restore_ncos(Chain::Tx)?;
        self.update_tx_bandwidth()?;
        debug!(hz, nco_offset_hz = self.tx.lo_offset_hz, "TX tuned");
        Ok(())
    }

    /// Routes the TX path for `band` (`_usdr_set_lna_tx`): the LMS6002D's amplifier, then,
    /// if that write succeeds, the antenna switch.
    pub(super) fn select_tx_band(&mut self, band: TxBand) -> Result<(), Error> {
        let bus = self.bus.as_mut();
        self.lms.select_pa(bus, band.amplifier())?;
        Gpo::TxSwitch.set(bus, band.tx_switch())
    }

    /// Powers the LMS6002D's transmitter if it is off.
    fn power_tx(&mut self) -> Result<(), Error> {
        self.set_tx_power(true)
    }

    /// Powers the LMS6002D's transmitter on or off, if it is not already
    /// (`_usdr_pwr_state` for TX). Unlike the receiver's, libusdr records the new state and
    /// takes the 25 ms settling wait even when the write fails, and so does this.
    pub(super) fn set_tx_power(&mut self, on: bool) -> Result<(), Error> {
        if self.tx.powered == on {
            return Ok(());
        }
        let bus = self.bus.as_mut();
        let switched = self.lms.set_tx_enabled(bus, on);
        self.tx.powered = on;
        bus.sleep(Duration::from_millis(25));
        switched
    }

    /// Tunes the RX PLL to `lo` (`lms6002d_tune_pll`).
    pub(super) fn tune_rx_pll(&mut self, lo: u32) -> Result<CapacitorWindow, Error> {
        self.lms.tune_rx(self.bus.as_mut(), lo, self.reference.hz())
    }

    /// Tunes the TX PLL to `lo` (`lms6002d_tune_pll`).
    pub(super) fn tune_tx_pll(&mut self, lo: u32) -> Result<CapacitorWindow, Error> {
        self.lms.tune_tx(self.bus.as_mut(), lo, self.reference.hz())
    }

    /// Tunes `chain`'s PLL to `lo`.
    pub(super) fn tune_pll(&mut self, chain: Chain, lo: u32) -> Result<CapacitorWindow, Error> {
        match chain {
            Chain::Rx => self.tune_rx_pll(lo),
            Chain::Tx => self.tune_tx_pll(lo),
        }
    }

    /// libusdr's fallback for an LO below 250 MHz `chain`'s PLL cannot lock at: tune the
    /// lowest LO that locks, found once by stepping up 1 MHz at a time and kept in
    /// `minimal_lo_hz` (0 until found), and return the gap for the NCOs, provided the
    /// converter rate leaves room for it. The minimal LO is kept even when the room check
    /// then fails, as libusdr keeps it.
    ///
    /// Both chains use the ADC rate and RX decimation: libusdr sets its DAC rate and TX
    /// interpolation (`dac_clk`, `txbb_intr`) to the same values on every rate change.
    fn extend_low_lo(
        &mut self,
        chain: Chain,
        lo: u32,
        minimal_lo_hz: &mut u32,
    ) -> Result<i32, Error> {
        if lo < Self::LOW_LO_HZ && *minimal_lo_hz == 0 {
            let mut sweep = lo;
            let mut result = Err(Error::PllUnlocked(lo));
            while sweep < Self::LOW_LO_HZ {
                result = self.tune_pll(chain, sweep);
                match result {
                    Err(Error::PllUnlocked(_)) => {}
                    Err(error) => return Err(error),
                    // A window this narrow at the bottom is not trusted; keep stepping.
                    Ok(window) if window.high >= 2 => break,
                    Ok(_) => {}
                }
                sweep += 1_000_000;
            }
            // The sweep's own error names its last LO; report the caller's.
            result.map_err(|error| match error {
                Error::PllUnlocked(_) => Error::PllUnlocked(lo),
                other => other,
            })?;
            *minimal_lo_hz = sweep;
            debug!(?chain, minimal_lo_hz = sweep, "lowest PLL lock found");
        } else if lo < Self::LOW_LO_HZ {
            self.tune_pll(chain, *minimal_lo_hz)?;
        } else {
            return Err(Error::PllUnlocked(lo));
        }

        let Some(decimation) = self.rx.decimation else {
            return Err(Error::PllUnlocked(lo));
        };
        let offset = i64::from(lo) - i64::from(*minimal_lo_hz);
        let baseband = self.adc_rate_hz / decimation.factor();
        #[expect(
            clippy::cast_possible_truncation,
            reason = "libusdr adds the 64-bit gap into a 32-bit unsigned"
        )]
        let needed = baseband.wrapping_add(offset.unsigned_abs() as u32);
        if f64::from(self.adc_rate_hz) * Self::NCO_HEADROOM < f64::from(needed) {
            return Err(Error::PllUnlocked(lo));
        }
        #[expect(
            clippy::cast_possible_truncation,
            reason = "libusdr stores the offset in a 32-bit int"
        )]
        let offset = offset as i32;
        Ok(offset)
    }

    /// Points both RX NCOs at the RX LO offset plus each channel's baseband offset
    /// (`usdr_restore_nco` for RX).
    pub(super) fn restore_rx_ncos(&mut self) -> Result<(), Error> {
        self.restore_ncos(Chain::Rx)
    }

    /// Points each of `chain`'s NCOs at the chain's LO offset plus that channel's baseband
    /// offset (`usdr_restore_nco`), in libusdr's wrapping `int` sum.
    pub(super) fn restore_ncos(&mut self, chain: Chain) -> Result<(), Error> {
        let (plan, offset_hz) = match chain {
            Chain::Rx => (self.rx.plan, self.rx.lo_offset_hz),
            Chain::Tx => (self.tx.plan, self.tx.lo_offset_hz),
        };
        let bus = self.bus.as_mut();
        for (channel, nco) in [Nco::Nco0, Nco::Nco1].into_iter().enumerate() {
            let hz = plan.baseband_hz(channel).wrapping_add(offset_hz);
            chain
                .phy()
                .set_nco(bus, nco, nco_word(hz, self.adc_rate_hz))?;
        }
        Ok(())
    }

    /// Sets the RX filter to the sample rate plus twice the LO offset and twice the
    /// largest baseband offset, unless the caller fixed it or no rate is set yet
    /// (`_usdr_update_bandwidth` for RX).
    pub(super) fn update_rx_bandwidth(&mut self) -> Result<(), Error> {
        if self.rx.fixed_bandwidth_hz.is_some() {
            return Ok(());
        }
        let Some(bandwidth) = self.following_bandwidth(
            self.rx.decimation,
            self.rx.lo_offset_hz,
            self.rx.plan.nco_distance_hz(),
        ) else {
            return Ok(());
        };
        self.lms.set_rx_bandwidth(self.bus.as_mut(), bandwidth)
    }

    /// Sets the TX filter as [`Self::update_rx_bandwidth`] sets the RX one, from the TX LO
    /// and baseband offsets (`_usdr_update_bandwidth` for TX).
    pub(super) fn update_tx_bandwidth(&mut self) -> Result<(), Error> {
        if self.tx.fixed_bandwidth_hz.is_some() {
            return Ok(());
        }
        let Some(bandwidth) = self.following_bandwidth(
            self.tx.interpolation,
            self.tx.lo_offset_hz,
            self.tx.plan.nco_distance_hz(),
        ) else {
            return Ok(());
        };
        self.lms.set_tx_bandwidth(self.bus.as_mut(), bandwidth)
    }

    /// The bandwidth a filter that follows the sample rate gets with the chain's rate
    /// factor `factor` (RX decimation, TX interpolation), LO offset `offset_hz` and NCO
    /// distance `distance_hz`: the converter rate over the factor, plus twice each offset,
    /// in libusdr's wrapping 32-bit arithmetic. `None` before any rate is set, where
    /// libusdr divides by zero.
    fn following_bandwidth(
        &self,
        factor: Option<Decimation>,
        offset_hz: i32,
        distance_hz: u32,
    ) -> Option<u32> {
        let rate = self.adc_rate_hz / factor?.factor();
        Some(
            distance_hz
                .wrapping_mul(2)
                .wrapping_add(rate)
                .wrapping_add(offset_hz.wrapping_mul(2).unsigned_abs()),
        )
    }
}

/// The NCO frequency word for `offset_hz` at an ADC rate of `adc_hz` (`_usdr_set_nco`).
///
/// libusdr's arithmetic: the offset as a fraction of the ADC rate, clamped to -1 when
/// outside ±1 whatever its sign, scaled by `INT_MAX` with truncation, then shifted left one
/// bit with wraparound. With no rate set the fraction is NaN, which libusdr converts with
/// undefined behaviour; on x86 that, shifted, gives 0, as Rust's conversion does.
pub(super) fn nco_word(offset_hz: i32, adc_hz: u32) -> i32 {
    let mut fraction = f64::from(offset_hz) / f64::from(adc_hz);
    if !(-1.0..=1.0).contains(&fraction) && !fraction.is_nan() {
        fraction = -1.0;
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "libusdr's double-to-int32_t conversion truncates; the value is in range"
    )]
    let word = (fraction * f64::from(i32::MAX)) as i32;
    word.wrapping_shl(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bands_split_at_230_mhz_and_2_8_ghz() {
        assert_eq!(RxBand::for_frequency(229_999_999), RxBand::Mixer);
        assert_eq!(RxBand::for_frequency(230_000_000), RxBand::Wide);
        assert_eq!(RxBand::for_frequency(2_799_999_999), RxBand::Wide);
        assert_eq!(RxBand::for_frequency(2_800_000_000), RxBand::High);
    }

    #[test]
    fn tx_bands_split_at_2_8_ghz_onto_pa1_and_pa2() {
        assert_eq!(TxBand::for_frequency(0), TxBand::Wide);
        assert_eq!(TxBand::for_frequency(2_799_999_999), TxBand::Wide);
        assert_eq!(TxBand::for_frequency(2_800_000_000), TxBand::High);
        // libusdr's `cfg_auto_tx`: PA1 on switch 1, PA2 on switch 0.
        assert_eq!(
            (TxBand::Wide.amplifier(), TxBand::Wide.tx_switch()),
            (PowerAmp::Pa1, 1)
        );
        assert_eq!(
            (TxBand::High.amplifier(), TxBand::High.tx_switch()),
            (PowerAmp::Pa2, 0)
        );
    }

    #[test]
    fn nco_word_is_the_scaled_fraction_shifted_left() {
        assert_eq!(nco_word(0, 32_000_000), 0);
        // -5 MHz of 32 MHz: -0.15625 * INT_MAX = -335544319, then doubled.
        assert_eq!(nco_word(-5_000_000, 32_000_000), -671_088_638);
    }

    #[test]
    fn out_of_range_nco_offsets_clamp_to_minus_one_whatever_their_sign() {
        // -INT_MAX shifted left wraps to 2.
        assert_eq!(nco_word(40_000_000, 32_000_000), 2);
        assert_eq!(nco_word(-40_000_000, 32_000_000), 2);
    }

    #[test]
    fn with_no_rate_the_nco_word_is_zero() {
        assert_eq!(nco_word(0, 0), 0);
    }
}
