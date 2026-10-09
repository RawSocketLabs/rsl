//! Setting the RX sample rate: `usdr_set_samplerate_ex` in `device/m2_lm6_1/usdr_ctrl.c`,
//! as `dev_m2_lm6_1_rate_m_set` reaches it with `{rate, 0, 0, 0}` (what the FFI `usdr`
//! crate sends).
//!
//! The ADC runs at the requested rate times a decimation factor, chosen to keep it between
//! 30.72 and 60 MS/s; the FPGA's decimator brings it back down. The Si5332 makes the sample
//! clock at twice the ADC rate, the link rate.

use std::time::Duration;

use super::Board;
use super::board::Chains;
use super::tune::RxBand;
use crate::chips::lp8758::Buck;
use crate::chips::si5332::Layout;
use crate::error::Error;
use crate::fpga::{Decimation, Nco, Phy};

impl Board {
    /// Highest sample rate, in S/s (`RATE_MAX`).
    const RATE_MAX: u32 = 80_000_000;

    /// Lowest sample rate on a board with only one chain, in S/s (`RATE_MIN`). With both,
    /// libusdr's floor is 31.25 kS/s, but the decimation search rejects anything below
    /// 960 kS/s first; see [`Self::decimation_for`].
    const RATE_MIN: u32 = 1_000_000;

    /// The ADC rate decimation aims for at least, in S/s (`TARGET_RATE`).
    const ADC_TARGET: u32 = 30_720_000;

    /// Decimation steps back one factor rather than run the ADC above this, in S/s
    /// (`SAFE_RATE`).
    const ADC_SAFE: u32 = 60_000_000;

    /// ADC rates from here up need the 1.925 V I/O rail, in S/s.
    const VIO_BOOST_FROM: u32 = 62_000_000;

    /// Sets the RX sample rate, in samples per second.
    ///
    /// libusdr's quirks are kept where they reach the registers: it powers the receiver,
    /// pulses the TX chain's reset and sets the TX NCOs even for an RX-only rate.
    ///
    /// Divergence on failure only: libusdr records the boosted rail, the powered receiver
    /// and the decimation even when a write fails; this records each only once its writes
    /// succeed, so a retry reissues them.
    pub(crate) fn set_rx_sample_rate(&mut self, rate: u32) -> Result<(), Error> {
        // The block size and decimation are fixed while a stream runs; libusdr's callers only
        // set the rate before creating one.
        if self.rx_stream.is_some() {
            return Err(Error::AlreadyStreaming);
        }
        let decimation =
            Self::decimation_for(rate, self.chains).ok_or(Error::UnsupportedSampleRate(rate))?;
        let adc_rate = rate * decimation.factor();
        // The sample clock runs at the link rate, twice the ADC rate.
        let layout = Layout::new(Self::REFERENCE_HZ, adc_rate * 2)
            .ok_or(Error::UnsupportedSampleRate(rate))?;
        let bus = self.bus.as_mut();

        let boost = adc_rate >= Self::VIO_BOOST_FROM;
        if boost != self.vio_boosted {
            let voltage = if boost {
                Self::LMS_VIO_BOOST
            } else {
                Self::LMS_VIO_NORMAL
            };
            Self::PMIC.set_voltage(bus, Buck::B3, voltage)?;
            self.vio_boosted = boost;
        }

        self.power_rx()?;
        let bus = self.bus.as_mut();
        Self::CLOCK.set_layout(
            bus,
            &layout,
            Self::lvpecl(self.rev3),
            Self::MIXER_LO_DIVIDER,
        )?;
        self.mixer_lo_hz = layout.vco_hz() / u32::from(Self::MIXER_LO_DIVIDER);

        if self.chains.rx && self.rx.decimation != Some(decimation) {
            Phy::Rx.reset(bus, 15)?;
            bus.sleep(Duration::from_micros(1));
            Phy::Rx.reset(bus, 9)?;
            Phy::load_rx_fir(bus, decimation)?;
        }
        self.rx.decimation = Some(decimation);
        self.adc_rate_hz = adc_rate;

        // Front-end reset, both chains.
        Phy::Rx.reset(bus, 1)?;
        Phy::Tx.reset(bus, 7)?;
        bus.sleep(Duration::from_micros(10));
        Phy::Rx.reset(bus, 0)?;
        Phy::Tx.reset(bus, 0)?;
        Phy::Rx.set_dc_correction(bus, true)?;

        // `usdr_restore_nco`, RX then TX. TX has no LO offset, so its words are 0.
        self.restore_rx_ncos()?;
        let bus = self.bus.as_mut();
        for nco in [Nco::Nco0, Nco::Nco1] {
            Phy::Tx.set_nco(bus, nco, 0)?;
        }

        // `USDR_SAMPLERATE_CHANGED`: the mixer LO moved with the clocks, so retune.
        if self.rx.mixer_on && self.rx.band == Some(RxBand::Mixer) {
            self.tune_rx_pll(self.mixer_lo_hz.wrapping_add(self.rx.lo_hz))?;
        }
        self.update_rx_bandwidth()
    }

    /// Fixes the RX filter bandwidth in Hz, or with 0 returns it to following the sample
    /// rate (`usdr_rfic_bb_set_badwidth`). Either way the filter is written now: for 0,
    /// like libusdr, that is its narrowest setting, until the next rate or frequency
    /// change.
    pub(crate) fn set_rx_bandwidth(&mut self, hz: u32) -> Result<(), Error> {
        self.rx.bandwidth_fixed = hz != 0;
        self.lms.set_rx_bandwidth(self.bus.as_mut(), hz)
    }

    /// The decimation libusdr picks for `rate`, or `None` for a rate it rejects.
    ///
    /// Only a board with both chains decimates: the smallest factor that brings the ADC
    /// to [`Self::ADC_TARGET`], one smaller if that overshoots [`Self::ADC_SAFE`].
    ///
    /// Intentional divergence: below 960 kS/s no factor reaches the target, and libusdr
    /// reads past the end of its factor table (undefined behaviour). Those rates are
    /// rejected here.
    fn decimation_for(rate: u32, chains: Chains) -> Option<Decimation> {
        if rate > Self::RATE_MAX {
            return None;
        }
        if !(chains.rx && chains.tx) {
            return (rate >= Self::RATE_MIN).then_some(Decimation::X1);
        }
        let reaching = Decimation::ALL
            .iter()
            .position(|factor| rate * factor.factor() >= Self::ADC_TARGET)?;
        let overshoots = rate * Decimation::ALL[reaching].factor() > Self::ADC_SAFE;
        let chosen = if overshoots && reaching > 0 {
            reaching - 1
        } else {
            reaching
        };
        Some(Decimation::ALL[chosen])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both chains, as every uSDR the oracle models has.
    const BOTH: Chains = Chains { rx: true, tx: true };
    /// An RX-only gateware.
    const RX_ONLY: Chains = Chains {
        rx: true,
        tx: false,
    };

    #[test]
    fn rates_below_960k_are_rejected_where_libusdr_overruns_its_table() {
        assert_eq!(Board::decimation_for(960_000, BOTH), Some(Decimation::X32));
        assert_eq!(Board::decimation_for(959_999, BOTH), None);
    }

    #[test]
    fn rates_above_80m_are_rejected() {
        assert_eq!(
            Board::decimation_for(80_000_000, BOTH),
            Some(Decimation::X1)
        );
        assert_eq!(Board::decimation_for(80_000_001, BOTH), None);
    }

    #[test]
    fn decimation_steps_back_rather_than_overshoot_60m() {
        // 1.9 MS/s x 32 = 60.8 MS/s overshoots, so x16 (30.4 MS/s).
        assert_eq!(
            Board::decimation_for(1_900_000, BOTH),
            Some(Decimation::X16)
        );
        // 15 MS/s x 4 = 60 MS/s is not above the limit.
        assert_eq!(
            Board::decimation_for(15_000_000, BOTH),
            Some(Decimation::X4)
        );
    }

    #[test]
    fn a_single_chain_board_never_decimates_and_starts_at_1m() {
        assert_eq!(
            Board::decimation_for(1_000_000, RX_ONLY),
            Some(Decimation::X1)
        );
        assert_eq!(Board::decimation_for(999_999, RX_ONLY), None);
    }
}
