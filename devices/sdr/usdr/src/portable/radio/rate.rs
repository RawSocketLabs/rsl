//! Setting the sample rate: `usdr_set_samplerate_ex` in `device/m2_lm6_1/usdr_ctrl.c`, as
//! `dev_m2_lm6_1_rate_m_set` reaches it with `{rx, tx, 0, 0}` and `dev_m2_lm6_1_rate_set`
//! (`/dm/rate/master`) with the same rate for both chains.
//!
//! The board has one sample clock. The ADC and DAC run at the requested rate times a rate
//! factor, chosen to keep them between 30.72 and 60 MS/s; the FPGA's decimator and
//! interpolator bring it back down and up. The Si5332 makes the sample clock at twice the
//! converter rate, the link rate. A rate for one chain reloads only that chain's FIR and
//! filter; the other chain's rate follows the clock all the same.

use core::time::Duration;

use super::super::clock::Layout;
use super::super::fpga::{Decimation, FPGA, FirLoad, Phy};
use super::super::lms6002d::{PllTune, TuneFault};
use super::super::lp8758::{self, Buck};
use super::super::register::IndexedRegister;
use super::super::script::Script;
use super::super::sequence::RegisterAccess;
use super::super::si5332::ClockSequence;
use super::super::si5332::sequence::LAYOUT_WRITES;
use super::power::ChainPower;
use super::run::{Run, Steps, owns_radio, parts};
use super::state::{Chain, Chains, Radio, RxBand};
use super::wiring;

/// The chains a sample-rate change is for: the nonzero entries of libusdr's `{rx, tx}`
/// rates. The board's one sample clock serves both either way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RateChains {
    /// The RX chain (`{rate, 0}`).
    Rx,

    /// The TX chain (`{0, rate}`).
    Tx,

    /// Both (`{rate, rate}`, `/dm/rate/master`).
    Both,
}

impl RateChains {
    /// The RX entry is set.
    const fn rx(self) -> bool {
        matches!(self, Self::Rx | Self::Both)
    }

    /// The TX entry is set.
    const fn tx(self) -> bool {
        matches!(self, Self::Tx | Self::Both)
    }
}

/// Highest sample rate, in S/s (`RATE_MAX`).
const RATE_MAX: u32 = 80_000_000;

/// Lowest sample rate on a board with only one chain, in S/s (`RATE_MIN`). With both,
/// libusdr's floor is 31.25 kS/s, but the decimation search rejects anything below
/// 960 kS/s first; see [`decimation_for`].
const RATE_MIN: u32 = 1_000_000;

/// The ADC rate decimation aims for at least, in S/s (`TARGET_RATE`).
const ADC_TARGET: u32 = 30_720_000;

/// Decimation steps back one factor rather than run the ADC above this, in S/s
/// (`SAFE_RATE`).
const ADC_SAFE: u32 = 60_000_000;

/// ADC rates from here up need the 1.925 V I/O rail, in S/s.
const VIO_BOOST_FROM: u32 = 62_000_000;

/// The decimation libusdr picks for `rate`, or `None` for a rate it rejects.
///
/// Only a board with both chains decimates: the smallest factor that brings the ADC to
/// [`ADC_TARGET`], one smaller if that overshoots [`ADC_SAFE`].
///
/// Intentional divergence: below 960 kS/s no factor reaches the target, and libusdr reads
/// past the end of its factor table (undefined behaviour). Those rates are rejected here.
fn decimation_for(rate: u32, chains: Chains) -> Option<Decimation> {
    if rate > RATE_MAX {
        return None;
    }
    if !(chains.rx && chains.tx) {
        return (rate >= RATE_MIN).then_some(Decimation::X1);
    }
    let reaching = Decimation::ALL
        .iter()
        .position(|factor| rate * factor.factor() >= ADC_TARGET)?;
    let overshoots = rate * Decimation::ALL[reaching].factor() > ADC_SAFE;
    let chosen = if overshoots && reaching > 0 {
        reaching - 1
    } else {
        reaching
    };
    Some(Decimation::ALL[chosen])
}

parts! {
    /// A sample-rate change's part.
    #[derive(Clone, Copy, Debug)]
    pub(crate) enum RatePart {
        /// The receiver's power.
        Power(Run<ChainPower>),
        /// The sample clocks.
        Clock(ClockSequence<LAYOUT_WRITES>),
        /// A FIR load.
        Fir(FirLoad),
        /// The mixer band's retune.
        Pll(PllTune),
        /// Writes and waits.
        Script(Script),
    }
}

/// Where a rate change is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    /// Not started.
    Start,
    /// The I/O rail.
    Rail,
    /// The receiver's power.
    Power,
    /// The clocks.
    Clock,
    /// The RX chain's resets before its FIR.
    RxReset,
    /// The RX FIR.
    RxFir,
    /// The TX chain's resets before its FIR.
    TxReset,
    /// The TX FIR.
    TxFir,
    /// The front-end resets.
    FrontEnd,
    /// Both chains' NCOs.
    Ncos,
    /// The mixer band's retune.
    Retune,
    /// The RX filter.
    RxBandwidth,
    /// The TX filter.
    TxBandwidth,
    /// Finished.
    Done,
}

/// Sets the sample rate for some chains.
///
/// libusdr's quirks are kept where they reach the registers: it powers the receiver and
/// pulses both chains' front-end resets for any rate, sets both chains' NCOs at the new
/// rate, and records the rate factor for both chains, so a chain whose FIR a rate did not
/// load keeps the old one until a rate for it changes the factor. Only a chain the rate is
/// for gets its filter following the new rate. The mixer LO moves with the clocks, so in
/// the mixer band the receiver retunes (`USDR_SAMPLERATE_CHANGED`).
///
/// Divergence on failure only: libusdr records the boosted rail, the powered receiver and
/// the rate factors even when a write fails; this records each only once its writes
/// succeed, so a retry reissues them.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SampleRate {
    /// The state.
    pub(crate) radio: Radio,
    /// The chains the rate is for.
    chains: RateChains,
    /// The rate factor.
    factor: Decimation,
    /// The ADC rate, in S/s.
    adc_rate_hz: u32,
    /// The sample-clock plan.
    layout: Layout,
    /// The ADC rate needs the boosted I/O rail.
    boost: bool,
    /// Where it is.
    stage: Stage,
    /// The retune's fault.
    fault: Option<TuneFault>,
}

impl SampleRate {
    /// The change to `rate` S/s for `chains`, or `None` for a rate the board cannot run
    /// (outside 960 kS/s to 80 MS/s, or 1 to 80 MS/s with one DSP chain, or with no
    /// sample-clock plan), before any I/O.
    pub(crate) fn new(radio: Radio, rate: u32, chains: RateChains) -> Option<Self> {
        let factor = decimation_for(rate, radio.chains)?;
        let adc_rate_hz = rate * factor.factor();
        // The sample clock runs at the link rate, twice the ADC rate.
        let layout = Layout::new(radio.reference_hz, adc_rate_hz * 2)?;
        Some(Self {
            radio,
            chains,
            factor,
            adc_rate_hz,
            layout,
            boost: adc_rate_hz >= VIO_BOOST_FROM,
            stage: Stage::Start,
            fault: None,
        })
    }

    /// The rate factor chosen.
    pub(crate) const fn factor(&self) -> Decimation {
        self.factor
    }

    /// The ADC rate, in S/s.
    pub(crate) const fn adc_rate_hz(&self) -> u32 {
        self.adc_rate_hz
    }
}

impl Steps for SampleRate {
    type Part = RatePart;

    #[expect(
        clippy::too_many_lines,
        reason = "one match over usdr_set_samplerate_ex's steps, in its order"
    )]
    fn next_part(&mut self, _failed: bool) -> Option<RatePart> {
        let radio = &mut self.radio;
        loop {
            let (stage, part) = match self.stage {
                Stage::Start => {
                    let voltage = if self.boost {
                        wiring::LMS_VIO_BOOST
                    } else {
                        wiring::LMS_VIO_NORMAL
                    };
                    let write = RegisterAccess::write(wiring::PMIC, voltage.entry_at(Buck::B3));
                    let rail = Script::new().i2c(lp8758::NAME, write);
                    (
                        Stage::Rail,
                        (self.boost != radio.vio_boosted).then_some(RatePart::Script(rail)),
                    )
                }
                Stage::Rail => {
                    radio.vio_boosted = self.boost;
                    let power = ChainPower::new(*radio, Chain::Rx, true);
                    (Stage::Power, Some(RatePart::Power(Run::new(power))))
                }
                Stage::Power => {
                    let clocks = ClockSequence::layout(
                        wiring::CLOCK,
                        &self.layout,
                        wiring::lvpecl(radio.rev3),
                        wiring::MIXER_LO_DIVIDER,
                    );
                    (Stage::Clock, Some(RatePart::Clock(clocks)))
                }
                Stage::Clock => {
                    radio.mixer_lo_hz = self.layout.vco_hz() / u32::from(wiring::MIXER_LO_DIVIDER);
                    let load = radio.chains.rx
                        && self.chains.rx()
                        && radio.rx.decimation != Some(self.factor);
                    let resets = Script::new()
                        .fpga(FPGA, Phy::Rx.reset(15))
                        .sleep(Duration::from_micros(1))
                        .fpga(FPGA, Phy::Rx.reset(9));
                    (Stage::RxReset, load.then_some(RatePart::Script(resets)))
                }
                Stage::RxReset => {
                    let load = radio.chains.rx
                        && self.chains.rx()
                        && radio.rx.decimation != Some(self.factor);
                    (
                        Stage::RxFir,
                        load.then_some(RatePart::Fir(FirLoad::rx(self.factor))),
                    )
                }
                Stage::RxFir => {
                    let load = radio.chains.tx
                        && self.chains.tx()
                        && radio.tx.interpolation != Some(self.factor);
                    let resets = Script::new()
                        .fpga(FPGA, Phy::Tx.reset(7))
                        .sleep(Duration::from_micros(1))
                        .fpga(FPGA, Phy::Tx.reset(0));
                    (Stage::TxReset, load.then_some(RatePart::Script(resets)))
                }
                Stage::TxReset => {
                    let load = radio.chains.tx
                        && self.chains.tx()
                        && radio.tx.interpolation != Some(self.factor);
                    (
                        Stage::TxFir,
                        load.then_some(RatePart::Fir(FirLoad::tx(self.factor))),
                    )
                }
                Stage::TxFir => {
                    radio.rx.decimation = Some(self.factor);
                    radio.tx.interpolation = Some(self.factor);
                    radio.adc_rate_hz = self.adc_rate_hz;
                    // Front-end reset, both chains.
                    let resets = Script::new()
                        .fpga(FPGA, Phy::Rx.reset(1))
                        .fpga(FPGA, Phy::Tx.reset(7))
                        .sleep(Duration::from_micros(10))
                        .fpga(FPGA, Phy::Rx.reset(0))
                        .fpga(FPGA, Phy::Tx.reset(0))
                        .fpga(FPGA, Phy::Rx.dc_correction(true));
                    (Stage::FrontEnd, Some(RatePart::Script(resets)))
                }
                Stage::FrontEnd => {
                    // `usdr_restore_nco`, RX then TX.
                    let ncos = radio
                        .restore_ncos(Chain::Rx)
                        .extend(&radio.restore_ncos(Chain::Tx));
                    (Stage::Ncos, Some(RatePart::Script(ncos)))
                }
                Stage::Ncos => {
                    let retune = radio.rx.mixer_on && radio.rx.band == Some(RxBand::Mixer);
                    if retune {
                        radio.rx.pll_lo_hz = radio.mixer_lo_hz.wrapping_add(radio.rx.lo_hz);
                    }
                    let tune = retune
                        .then(|| RatePart::Pll(radio.tune_pll(Chain::Rx, radio.rx.pll_lo_hz)));
                    (Stage::Retune, tune)
                }
                Stage::Retune => {
                    let bandwidth = self.chains.rx().then(|| radio.update_bandwidth(Chain::Rx));
                    (
                        Stage::RxBandwidth,
                        bandwidth.flatten().map(RatePart::Script),
                    )
                }
                Stage::RxBandwidth => {
                    let bandwidth = self.chains.tx().then(|| radio.update_bandwidth(Chain::Tx));
                    (
                        Stage::TxBandwidth,
                        bandwidth.flatten().map(RatePart::Script),
                    )
                }
                Stage::TxBandwidth | Stage::Done => (Stage::Done, None),
            };
            self.stage = stage;
            if part.is_some() || stage == Stage::Done {
                return part;
            }
        }
    }

    fn absorb(&mut self, part: &RatePart) {
        match part {
            RatePart::Power(run) => self.radio = run.steps().radio,
            RatePart::Pll(tune) => {
                self.radio.lms = *tune.chip();
                if let Some(Err(fault)) = tune.outcome() {
                    self.fault = Some(fault);
                    self.stage = Stage::Done;
                }
            }
            RatePart::Clock(_) | RatePart::Fir(_) | RatePart::Script(_) => {}
        }
    }

    fn fault(&self) -> Option<TuneFault> {
        self.fault
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both chains, as every uSDR the oracle models has.
    const BOTH: Chains = Chains {
        rx: true,
        tx: true,
        extended_tx_core: false,
    };
    /// An RX-only gateware.
    const RX_ONLY: Chains = Chains {
        rx: true,
        tx: false,
        extended_tx_core: false,
    };

    #[test]
    fn rates_below_960k_are_rejected_where_libusdr_overruns_its_table() {
        assert_eq!(decimation_for(960_000, BOTH), Some(Decimation::X32));
        assert_eq!(decimation_for(959_999, BOTH), None);
    }

    #[test]
    fn rates_above_80m_are_rejected() {
        assert_eq!(decimation_for(80_000_000, BOTH), Some(Decimation::X1));
        assert_eq!(decimation_for(80_000_001, BOTH), None);
    }

    #[test]
    fn decimation_steps_back_rather_than_overshoot_60m() {
        // 1.9 MS/s x 32 = 60.8 MS/s overshoots, so x16 (30.4 MS/s).
        assert_eq!(decimation_for(1_900_000, BOTH), Some(Decimation::X16));
        // 15 MS/s x 4 = 60 MS/s is not above the limit.
        assert_eq!(decimation_for(15_000_000, BOTH), Some(Decimation::X4));
    }

    #[test]
    fn a_single_chain_board_never_decimates_and_starts_at_1m() {
        assert_eq!(decimation_for(1_000_000, RX_ONLY), Some(Decimation::X1));
        assert_eq!(decimation_for(999_999, RX_ONLY), None);
    }
}

owns_radio!(SampleRate);
