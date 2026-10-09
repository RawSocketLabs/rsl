//! Tuning the receiver: `usdr_rfic_fe_set_freq` for RX in `device/m2_lm6_1/usdr_ctrl.c`,
//! as `/dm/sdr/0/rx/freqency` reaches it.
//!
//! The frequency picks a band: below 230 MHz the board mixer adds the Si5332's mixer LO
//! (output 3) to lift the signal into LNA3's range, so the LMS6002D tunes that much higher;
//! up to 2.8 GHz LNA1 takes it directly, and LNA2 above. The RX PLL then tunes. Below
//! 250 MHz, if it cannot lock, libusdr finds the lowest LO that does and makes up the
//! difference with the NCO, if the sample rate leaves room.

use std::time::Duration;

use super::Board;
use crate::chips::lms6002d::{CapacitorWindow, Lna};
use crate::error::Error;
use crate::fpga::{Gpo, Nco, Phy};

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
    fn for_frequency(hz: u32) -> Self {
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

impl Board {
    /// Below this LO, in Hz, a PLL that cannot lock falls back to the lowest LO that can,
    /// plus an NCO offset (`USDR_LO_LOW_RANGE`).
    const LOW_LO_HZ: u32 = 250_000_000;

    /// The share of the ADC rate an NCO offset plus the signal may occupy.
    const NCO_HEADROOM: f64 = 0.45;

    /// Tunes the receiver to `hz`.
    ///
    /// Divergences, both where libusdr would divide by zero before any sample rate is set:
    /// the NCO lock fallback fails with [`Error::PllUnlocked`], and the filter is left
    /// alone. On failure, the band change leaves state as libusdr does (see
    /// [`Self::power_rx_and_select_band`]), but this call records the LO offset and the
    /// minimal LO only once their writes succeed.
    pub(crate) fn set_rx_frequency(&mut self, hz: u32) -> Result<(), Error> {
        self.rx.lo_hz = hz;
        // `_usdr_signal_event(USDR_RX_LO_CHANGED)`; libusdr discards its result.
        let _ = self.power_rx_and_select_band(RxBand::for_frequency(hz));

        self.rx.lo_offset_hz = 0;
        let lo = if self.rx.mixer_on {
            hz.wrapping_add(self.mixer_lo_hz)
        } else {
            hz
        };
        let mut tuned = self.tune_rx_pll(lo);
        if matches!(tuned, Err(Error::PllUnlocked(_))) {
            // libusdr: the LDO may not be ready yet.
            self.bus.sleep(Duration::from_millis(5));
            tuned = self.tune_rx_pll(lo);
        }
        match tuned {
            Err(Error::PllUnlocked(_)) => self.extend_low_lo(lo)?,
            other => {
                other?;
            }
        }

        self.restore_rx_ncos()?;
        self.update_rx_bandwidth()
    }

    /// Powers the receiver if it is off, then routes the RX path for `band`
    /// (`_usdr_pwr_state`, `_usdr_set_lna_rx`, `usdr_set_rx_port_switch`).
    ///
    /// Error handling follows libusdr's: a power-up failure is ignored, a failed LNA
    /// selection stops before the switches, and the mixer state is recorded even when a
    /// switch write fails, though the Si5332 is then not touched.
    fn power_rx_and_select_band(&mut self, band: RxBand) -> Result<(), Error> {
        let _ = self.power_rx();
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
        if self.rx.powered {
            return Ok(());
        }
        let bus = self.bus.as_mut();
        self.lms.set_rx_enabled(bus, true)?;
        self.rx.powered = true;
        self.lms.enable_rx_vga2(bus)?;
        bus.sleep(Duration::from_millis(25));
        Ok(())
    }

    /// Tunes the RX PLL to `lo` (`lms6002d_tune_pll`).
    pub(super) fn tune_rx_pll(&mut self, lo: u32) -> Result<CapacitorWindow, Error> {
        self.lms.tune_rx(self.bus.as_mut(), lo, Self::REFERENCE_HZ)
    }

    /// libusdr's fallback for an LO below 250 MHz the PLL cannot lock at: tune the lowest
    /// LO that locks, found once by stepping up 1 MHz at a time, and record the gap for the
    /// NCO, provided the ADC rate leaves room for it.
    fn extend_low_lo(&mut self, lo: u32) -> Result<(), Error> {
        if lo < Self::LOW_LO_HZ && self.rx.minimal_lo_hz == 0 {
            let mut sweep = lo;
            let mut result = Err(Error::PllUnlocked(lo));
            while sweep < Self::LOW_LO_HZ {
                result = self.tune_rx_pll(sweep);
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
            self.rx.minimal_lo_hz = sweep;
        } else if lo < Self::LOW_LO_HZ {
            self.tune_rx_pll(self.rx.minimal_lo_hz)?;
        } else {
            return Err(Error::PllUnlocked(lo));
        }

        let Some(decimation) = self.rx.decimation else {
            return Err(Error::PllUnlocked(lo));
        };
        let offset = i64::from(lo) - i64::from(self.rx.minimal_lo_hz);
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
        self.rx.lo_offset_hz = offset;
        Ok(())
    }

    /// Points both RX NCOs at the LO offset (`usdr_restore_nco` for RX).
    pub(super) fn restore_rx_ncos(&mut self) -> Result<(), Error> {
        let word = nco_word(self.rx.lo_offset_hz, self.adc_rate_hz);
        let bus = self.bus.as_mut();
        for nco in [Nco::Nco0, Nco::Nco1] {
            Phy::Rx.set_nco(bus, nco, word)?;
        }
        Ok(())
    }

    /// Sets the RX filter to the sample rate plus twice the LO offset, unless the caller
    /// fixed it or no rate is set yet (`_usdr_update_bandwidth` for RX).
    pub(super) fn update_rx_bandwidth(&mut self) -> Result<(), Error> {
        if self.rx.bandwidth_fixed {
            return Ok(());
        }
        let Some(decimation) = self.rx.decimation else {
            return Ok(());
        };
        let bandwidth = (self.adc_rate_hz / decimation.factor())
            .wrapping_add(self.rx.lo_offset_hz.wrapping_mul(2).unsigned_abs());
        self.lms.set_rx_bandwidth(self.bus.as_mut(), bandwidth)
    }
}

/// The NCO frequency word for `offset_hz` at an ADC rate of `adc_hz` (`_usdr_set_nco`).
///
/// libusdr's arithmetic: the offset as a fraction of the ADC rate, clamped to -1 when
/// outside ±1 whatever its sign, scaled by `INT_MAX` with truncation, then shifted left one
/// bit with wraparound. With no rate set the fraction is NaN, which libusdr converts with
/// undefined behaviour; on x86 that, shifted, gives 0, as Rust's conversion does.
fn nco_word(offset_hz: i32, adc_hz: u32) -> i32 {
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
