//! What the board's RF sequences depend on and change: the LMS6002D's cache, each chain's
//! settings as libusdr tracks them, the rates and the gateware's chains. The blocking
//! [`Board`](crate) and a native handle each hold one [`Radio`] and lend a copy to every
//! machine, taking it back when the machine ends, failed or not.

use super::super::fpga::{Decimation, Phy};
use super::super::lms6002d::{Lms6002d, Lna, PowerAmp};

/// Which DSP chains the gateware has (HWID bits 25 and 24), and its TX stream core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Chains {
    /// An RX chain.
    pub(crate) rx: bool,

    /// A TX chain.
    pub(crate) tx: bool,

    /// The TX stream core is the extended one (`CORE_EXFETX_DMA32_R0_2`, HWID bit 26)
    /// rather than the original (`CORE_SFETX_DMA32_R0`).
    pub(crate) extended_tx_core: bool,
}

/// A powered board's RF state: the LMS6002D's cache and everything else the board's
/// sequences read and write.
#[derive(Clone, Copy, Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent flags, each one of libusdr's, not states of one machine"
)]
pub(crate) struct Radio {
    /// The RF transceiver's cache: its registers are not read back.
    pub(crate) lms: Lms6002d,

    /// Revision 3, where the Si5332's RX sample clock is output 1 rather than 0.
    pub(crate) rev3: bool,

    /// Which DSP chains the gateware has.
    pub(crate) chains: Chains,

    /// The reference every clock derives from, in Hz (libusdr's `fref`).
    pub(crate) reference_hz: u32,

    /// The LMS6002D I/O rail is boosted (1.925 V) rather than normal (1.8 V).
    pub(crate) vio_boosted: bool,

    /// The ADC's sample rate, the requested rate times the decimation (libusdr's
    /// `adc_clk`); 0 until a rate is set.
    pub(crate) adc_rate_hz: u32,

    /// The mixer LO: the Si5332 VCO over the mixer divider (libusdr's `mixer_lo`); 0 until a
    /// rate is set.
    pub(crate) mixer_lo_hz: u32,

    /// What the receive chain was last set to.
    pub(crate) rx: RxState,

    /// What the transmit chain was last set to.
    pub(crate) tx: TxState,

    /// libusdr's `rx_run`: set when an RX stream is created, cleared when it is destroyed,
    /// and left set by a failed create.
    pub(crate) rx_running: bool,

    /// libusdr's `tx_run`, the same for the TX stream.
    pub(crate) tx_running: bool,
}

impl Radio {
    /// A board just powered up: the LMS6002D as power-up left it, nothing set yet.
    pub(crate) const fn new(lms: Lms6002d, rev3: bool, chains: Chains, reference_hz: u32) -> Self {
        Self {
            lms,
            rev3,
            chains,
            reference_hz,
            vio_boosted: false,
            adc_rate_hz: 0,
            mixer_lo_hz: 0,
            rx: RxState::new(),
            tx: TxState::new(),
            rx_running: false,
            tx_running: false,
        }
    }

    /// The bandwidth a filter that follows the sample rate gets with the chain's rate
    /// factor `factor` (RX decimation, TX interpolation), LO offset `offset_hz` and NCO
    /// distance `distance_hz`: the converter rate over the factor, plus twice each offset,
    /// in libusdr's wrapping 32-bit arithmetic. `None` before any rate is set, where
    /// libusdr divides by zero.
    pub(crate) fn following_bandwidth(
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

/// The receive chain's settings that later calls depend on, as libusdr tracks them.
#[derive(Clone, Copy, Debug, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent flags, each one of libusdr's, not states of one machine"
)]
pub(crate) struct RxState {
    /// The LMS6002D's receiver and RXVGA2 are powered (libusdr's `rx_pwren`).
    pub(crate) powered: bool,

    /// The RX bandwidth the caller fixed, in Hz (libusdr's `rx_bw`), so sample-rate and
    /// frequency changes leave the filter alone; `None` while it follows the rate.
    pub(crate) fixed_bandwidth_hz: Option<u32>,

    /// The decimation of the last sample rate set (libusdr's `rxbb_decim`); `None` until the
    /// first. Its FIR is loaded unless the board has no RX chain.
    pub(crate) decimation: Option<Decimation>,

    /// The frequency the caller tuned to, in Hz (libusdr's `rx_lo`): the average of the
    /// NCO channels' LOs.
    pub(crate) lo_hz: u32,

    /// Each NCO channel's LO and baseband offset (libusdr's `rx_raw`).
    pub(crate) plan: FrequencyPlan,

    /// The LO the RX PLL was last asked for by a tune, in Hz: [`Self::lo_hz`] plus the
    /// mixer LO in the mixer band (libusdr's `rfic_rx_lo`). The calibration restores it.
    pub(crate) pll_lo_hz: u32,

    /// The band in use (libusdr's `rx_cfg_path`): the frequency's, unless the caller fixed
    /// one; `None` until either happens.
    pub(crate) band: Option<RxBand>,

    /// The caller fixed the band, so frequency changes leave it alone (libusdr's
    /// `rx_rfic_path` other than `USDR_RX_AUTO`).
    pub(crate) band_fixed: bool,

    /// The TX mixer's output is looped back into the RX path (libusdr's
    /// `rf_loopback_active`).
    pub(crate) rf_loopback: bool,

    /// The board mixer is in the RX path (libusdr's `mexir_en`).
    pub(crate) mixer_on: bool,

    /// The lowest LO the RX PLL was found to lock at below 250 MHz, in Hz (libusdr's
    /// `rx_minimal_lo`); 0 until a lock failure made the driver search for it.
    pub(crate) minimal_lo_hz: u32,

    /// How far the PLL sits from the wanted LO, in Hz, when it cannot reach it; the NCO
    /// makes up the difference (libusdr's `rx_exten_lo`).
    pub(crate) lo_offset_hz: i32,

    /// RXVGA1's gain above its 5 dB floor, in dB (libusdr's `rx_vga1`), which the TX
    /// calibrations restore and raise from.
    pub(crate) vga1_db: u8,

    /// RXVGA2's first-stage gain in 3 dB steps (libusdr's `rx_vga2a`), which the DC
    /// calibration at stream creation restores.
    pub(crate) vga2a: u8,

    /// RXVGA2's second-stage gain in 3 dB steps (libusdr's `rx_vga2b`).
    pub(crate) vga2b: u8,
}

impl RxState {
    /// Nothing set yet.
    const fn new() -> Self {
        Self {
            powered: false,
            fixed_bandwidth_hz: None,
            decimation: None,
            lo_hz: 0,
            plan: FrequencyPlan::new(),
            pll_lo_hz: 0,
            band: None,
            band_fixed: false,
            rf_loopback: false,
            mixer_on: false,
            minimal_lo_hz: 0,
            lo_offset_hz: 0,
            vga1_db: 0,
            vga2a: 0,
            vga2b: 0,
        }
    }
}

/// The transmit chain's settings that later calls depend on, as libusdr tracks them
/// (its `tx_*` fields).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TxState {
    /// The LMS6002D's transmitter is powered (libusdr's `tx_pwren`).
    pub(crate) powered: bool,

    /// The TX bandwidth the caller fixed, in Hz (libusdr's `tx_bw`), so frequency changes
    /// leave the filter alone; `None` while it follows the rate.
    pub(crate) fixed_bandwidth_hz: Option<u32>,

    /// The frequency the caller tuned to, in Hz (libusdr's `tx_lo`): the average of the
    /// NCO channels' LOs; 0 until tuned.
    pub(crate) lo_hz: u32,

    /// Each NCO channel's LO and baseband offset (libusdr's `tx_raw`).
    pub(crate) plan: FrequencyPlan,

    /// The interpolation of the last sample rate set (libusdr's `txbb_intr`); `None`
    /// until the first. Every rate change sets it, TX's or not, but only a TX rate loads
    /// its FIR, and only when it changes, so after an RX rate a TX rate of the same factor
    /// loads none, as in libusdr.
    pub(crate) interpolation: Option<Decimation>,

    /// The caller fixed the band, so frequency changes leave it alone (libusdr's
    /// `tx_rfic_path` other than `USDR_TX_AUTO`).
    pub(crate) band_fixed: bool,

    /// The lowest LO the TX PLL was found to lock at below 250 MHz, in Hz (libusdr's
    /// `tx_minimal_lo`); 0 until a lock failure made the driver search for it.
    pub(crate) minimal_lo_hz: u32,

    /// How far the PLL sits from the wanted LO, in Hz, when it cannot reach it; the TX NCOs
    /// make up the difference (libusdr's `tx_exten_lo`).
    pub(crate) lo_offset_hz: i32,
}

impl TxState {
    /// Nothing set yet.
    const fn new() -> Self {
        Self {
            powered: false,
            fixed_bandwidth_hz: None,
            lo_hz: 0,
            plan: FrequencyPlan::new(),
            interpolation: None,
            band_fixed: false,
            minimal_lo_hz: 0,
            lo_offset_hz: 0,
        }
    }
}

/// Which of a chain's two NCO channels a frequency call sets (libusdr's channel mask, from
/// the path's `/0` or `/1`, or both without one).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NcoChannels {
    /// Both channels (no suffix: mask 3).
    #[default]
    Both,

    /// NCO 0 (`/0`: mask 1). On RX it is the one every stream sees: its I and Q are the
    /// front end's lanes 0 and 1, so it also feeds the real channel-1 (Q lane) stream.
    Nco0,

    /// NCO 1 (`/1`: mask 2). No RX stream carries it; what it feeds on this gateware is
    /// not shown by the vendored source.
    Nco1,
}

impl NcoChannels {
    /// Whether each channel, 0 then 1, is selected.
    const fn selects(self) -> [bool; 2] {
        match self {
            Self::Both => [true, true],
            Self::Nco0 => [true, false],
            Self::Nco1 => [false, true],
        }
    }
}

/// One chain's per-channel frequencies (libusdr's `freq_data_t`): each channel's LO and
/// baseband offset, unset until a call sets them.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct FrequencyPlan {
    /// Each channel's LO, in Hz (`lo[i]`).
    lo_hz: [Option<u32>; 2],

    /// Each channel's NCO offset, in Hz (`bb[i]`).
    baseband_hz: [Option<i32>; 2],
}

impl FrequencyPlan {
    /// Nothing set.
    const fn new() -> Self {
        Self {
            lo_hz: [None; 2],
            baseband_hz: [None; 2],
        }
    }

    /// Sets the LO of `channels` to `hz`, then makes every channel with an LO offset by its
    /// distance from their average, and returns that average, the LO the PLL is to tune.
    ///
    /// libusdr's arithmetic: the average in `double`, truncated to `unsigned` for the
    /// chain's LO; each offset truncated towards zero to `int32_t`.
    pub(crate) fn set_lo(&mut self, channels: NcoChannels, hz: u32) -> u32 {
        for (lo, selected) in self.lo_hz.iter_mut().zip(channels.selects()) {
            if selected {
                *lo = Some(hz);
            }
        }
        let (sum, count) = self
            .lo_hz
            .iter()
            .flatten()
            .fold((0.0, 0_u8), |(sum, count), &lo| {
                (sum + f64::from(lo), count + 1)
            });
        let average = sum / f64::from(count);
        for (lo, baseband) in self.lo_hz.iter().zip(&mut self.baseband_hz) {
            if let Some(lo) = lo {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "libusdr truncates the double to int32_t; half the gap of two \
                              u32s fits"
                )]
                let offset = (f64::from(*lo) - average) as i32;
                *baseband = Some(offset);
            }
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "libusdr truncates the average of u32s, itself within u32, to unsigned"
        )]
        let average = average as u32;
        average
    }

    /// Sets the baseband offset of `channels` to `hz`.
    pub(crate) fn set_baseband(&mut self, channels: NcoChannels, hz: i32) {
        for (baseband, selected) in self.baseband_hz.iter_mut().zip(channels.selects()) {
            if selected {
                *baseband = Some(hz);
            }
        }
    }

    /// Channel `channel`'s baseband offset, 0 while unset, as libusdr's NCO restore reads
    /// it.
    pub(crate) fn baseband_hz(&self, channel: usize) -> i32 {
        self.baseband_hz[channel].unwrap_or(0)
    }

    /// The largest offset magnitude among set offsets (libusdr's `nco_distance`), which
    /// widens a filter that follows the rate by twice its value.
    pub(crate) fn nco_distance_hz(&self) -> u32 {
        self.baseband_hz
            .iter()
            .flatten()
            .map(|hz| hz.unsigned_abs())
            .max()
            .unwrap_or(0)
    }
}

/// A signal chain: which synthesizer, NCOs and filter an operation drives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Chain {
    /// The receive chain.
    Rx,

    /// The transmit chain.
    Tx,
}

impl Chain {
    /// The chain's FPGA configuration port, where its NCOs are.
    pub(crate) const fn phy(self) -> Phy {
        match self {
            Self::Rx => Phy::Rx,
            Self::Tx => Phy::Tx,
        }
    }
}

/// Where the RX path runs (libusdr's `cfg_auto_rx` on boards with the mixer, which every
/// supported revision has).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RxBand {
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
    pub(crate) fn for_frequency(hz: u32) -> Self {
        if hz < Self::MIXER_BELOW_HZ {
            Self::Mixer
        } else if hz < Self::WIDE_BELOW_HZ {
            Self::Wide
        } else {
            Self::High
        }
    }

    /// The LMS6002D input.
    pub(crate) fn lna(self) -> Lna {
        match self {
            Self::Mixer => Lna::Lna3,
            Self::Wide => Lna::Lna1,
            Self::High => Lna::Lna2,
        }
    }

    /// The RX antenna switch setting (`Gpo::RxSwitch`).
    pub(crate) fn rx_switch(self) -> u8 {
        match self {
            Self::Wide => 0,
            Self::Mixer | Self::High => 1,
        }
    }

    /// The board mixer is in the path.
    pub(crate) fn mixer(self) -> bool {
        self == Self::Mixer
    }
}

/// Where the TX path runs (libusdr's `cfg_auto_tx`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TxBand {
    /// Below 2.8 GHz: PA1 (`W`, also `B1`).
    Wide,

    /// 2.8 GHz and up: PA2 (`H`, also `B2`).
    High,
}

impl TxBand {
    /// The wide band ends here, in Hz.
    const WIDE_BELOW_HZ: u32 = 2_800_000_000;

    /// The band for a frequency (`get_antenna_cfg_by_freq`).
    pub(crate) fn for_frequency(hz: u32) -> Self {
        if hz < Self::WIDE_BELOW_HZ {
            Self::Wide
        } else {
            Self::High
        }
    }

    /// The LMS6002D output amplifier.
    pub(crate) fn amplifier(self) -> PowerAmp {
        match self {
            Self::Wide => PowerAmp::Pa1,
            Self::High => PowerAmp::Pa2,
        }
    }

    /// The TX antenna switch setting (`Gpo::TxSwitch`).
    pub(crate) fn tx_switch(self) -> u8 {
        match self {
            Self::Wide => 1,
            Self::High => 0,
        }
    }
}

/// The NCO frequency word for `offset_hz` at an ADC rate of `adc_hz` (`_usdr_set_nco`).
///
/// libusdr's arithmetic: the offset as a fraction of the ADC rate, clamped to -1 when
/// outside ±1 whatever its sign, scaled by `INT_MAX` with truncation, then shifted left one
/// bit with wraparound. With no rate set the fraction is NaN, which libusdr converts with
/// undefined behaviour; on x86 that, shifted, gives 0, as Rust's conversion does.
pub(crate) fn nco_word(offset_hz: i32, adc_hz: u32) -> i32 {
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
    fn a_whole_chain_tune_leaves_both_offsets_zero() {
        let mut plan = FrequencyPlan::default();
        assert_eq!(plan.set_lo(NcoChannels::Both, 1_000_000_000), 1_000_000_000);
        assert_eq!((plan.baseband_hz(0), plan.baseband_hz(1)), (0, 0));
        assert_eq!(plan.nco_distance_hz(), 0);
    }

    #[test]
    fn two_channel_los_centre_the_pll_and_split_the_gap() {
        let mut plan = FrequencyPlan::default();
        plan.set_lo(NcoChannels::Nco0, 1_000_000_000);
        // 1000 and 1000.000003 MHz: the average's half hertz is truncated, and so is each
        // offset, towards zero.
        assert_eq!(plan.set_lo(NcoChannels::Nco1, 1_000_000_003), 1_000_000_001);
        assert_eq!((plan.baseband_hz(0), plan.baseband_hz(1)), (-1, 1));
        assert_eq!(plan.nco_distance_hz(), 1);
    }

    #[test]
    fn a_lone_channel_lo_leaves_the_other_offset_unset() {
        let mut plan = FrequencyPlan::default();
        plan.set_baseband(NcoChannels::Nco0, -5_000);
        assert_eq!(plan.set_lo(NcoChannels::Nco1, 900_000_000), 900_000_000);
        assert_eq!(
            (plan.baseband_hz(0), plan.baseband_hz(1)),
            (-5_000, 0),
            "channel 0 keeps its offset: it has no LO to re-centre"
        );
        assert_eq!(plan.nco_distance_hz(), 5_000);
    }

    #[test]
    fn the_distance_is_the_largest_offset_magnitude() {
        let mut plan = FrequencyPlan::default();
        plan.set_baseband(NcoChannels::Both, 2_000);
        plan.set_baseband(NcoChannels::Nco1, i32::MIN);
        assert_eq!(plan.nco_distance_hz(), 1 << 31);
    }

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
