//! Tuning: `usdr_rfic_fe_set_freq` for the RX and TX LOs in `device/m2_lm6_1/usdr_ctrl.c`.
//!
//! The RX frequency picks a band: below 230 MHz the board mixer adds the Si5332's mixer LO
//! (output 3) to lift the signal into LNA3's range, so the LMS6002D tunes that much higher;
//! up to 2.8 GHz LNA1 takes it directly, and LNA2 above, unless the caller fixed the path.
//! The TX frequency picks PA1 up to 2.8 GHz and PA2 above, unless fixed. The chain's PLL
//! then tunes. Below 250 MHz, if it cannot lock, libusdr finds the lowest LO that does and
//! makes up the difference with the chain's NCOs, if the sample rate leaves room.

use core::time::Duration;

use super::super::fpga::{FPGA, Gpo, Nco};
use super::super::lms6002d::{CapacitorWindow, Pll, PllTune, TuneFault};
use super::super::register::Register;
use super::super::script::Script;
use super::super::sequence::{RegisterAccess, SequenceFailure};
use super::super::si5332;
use super::super::si5332::output::OutputEnables0to3;
use super::power::ChainPower;
use super::run::{Policy, Run, Steps, owns_radio, parts};
use super::state::{Chain, NcoChannels, Radio, RxBand, TxBand, nco_word};
use super::wiring;

/// Below this LO, in Hz, a PLL that cannot lock falls back to the lowest LO that can, plus
/// an NCO offset (`USDR_LO_LOW_RANGE`).
const LOW_LO_HZ: u32 = 250_000_000;

/// The share of the ADC rate an NCO offset plus the signal may occupy.
const NCO_HEADROOM: f64 = 0.45;

/// The sweep's step from one LO to the next, in Hz.
const SWEEP_STEP_HZ: u32 = 1_000_000;

impl Chain {
    /// The chain's synthesizer.
    pub(crate) const fn pll(self) -> Pll {
        match self {
            Self::Rx => Pll::Rx,
            Self::Tx => Pll::Tx,
        }
    }
}

impl Radio {
    /// A tuning of `chain`'s PLL to `lo_hz` (`lms6002d_tune_pll`).
    pub(crate) fn tune_pll(&self, chain: Chain, lo_hz: u32) -> PllTune {
        PllTune::new(self.lms, chain.pll(), lo_hz, self.reference_hz)
    }

    /// Points each of `chain`'s NCOs at the chain's LO offset plus that channel's baseband
    /// offset (`usdr_restore_nco`), in libusdr's wrapping `int` sum.
    pub(crate) fn restore_ncos(&self, chain: Chain) -> Script {
        let (plan, offset_hz) = match chain {
            Chain::Rx => (self.rx.plan, self.rx.lo_offset_hz),
            Chain::Tx => (self.tx.plan, self.tx.lo_offset_hz),
        };
        let mut script = Script::new();
        for (channel, nco) in [Nco::Nco0, Nco::Nco1].into_iter().enumerate() {
            let hz = plan.baseband_hz(channel).wrapping_add(offset_hz);
            for access in chain.phy().nco(nco, nco_word(hz, self.adc_rate_hz)) {
                script = script.fpga(FPGA, access);
            }
        }
        script
    }

    /// Sets `chain`'s filter to the sample rate plus twice the LO offset and twice the
    /// largest baseband offset, unless the caller fixed it or no rate is set yet
    /// (`_usdr_update_bandwidth`).
    pub(crate) fn update_bandwidth(&self, chain: Chain) -> Option<Script> {
        match chain {
            Chain::Rx if self.rx.fixed_bandwidth_hz.is_none() => {
                let hz = self.following_bandwidth(
                    self.rx.decimation,
                    self.rx.lo_offset_hz,
                    self.rx.plan.nco_distance_hz(),
                )?;
                Some(self.lms.set_rx_bandwidth(hz))
            }
            Chain::Tx if self.tx.fixed_bandwidth_hz.is_none() => {
                let hz = self.following_bandwidth(
                    self.tx.interpolation,
                    self.tx.lo_offset_hz,
                    self.tx.plan.nco_distance_hz(),
                )?;
                Some(self.lms.set_tx_bandwidth(hz))
            }
            Chain::Rx | Chain::Tx => None,
        }
    }

    /// Routes the TX path for `band` (`_usdr_set_lna_tx`): the LMS6002D's amplifier, then,
    /// if that write succeeds, the antenna switch.
    pub(crate) fn select_tx_band(&mut self, band: TxBand) -> Script {
        self.lms
            .select_pa(band.amplifier())
            .fpga(FPGA, Gpo::TxSwitch.write(band.tx_switch()))
    }
}

/// Routes the RX path for a band (`_usdr_set_lna_rx`, `usdr_set_rx_port_switch`).
///
/// Error handling follows libusdr's: the band is recorded first, a failed LNA selection
/// stops before the switches, and the mixer state is recorded even when a switch write
/// fails, though the Si5332 is then not touched.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SelectRxBand {
    /// The state.
    pub(crate) radio: Radio,
    /// The band.
    band: RxBand,
    /// Parts begun.
    stage: u8,
}

impl SelectRxBand {
    /// Routes the RX path for `band`.
    pub(crate) const fn new(radio: Radio, band: RxBand) -> Self {
        Self {
            radio,
            band,
            stage: 0,
        }
    }
}

impl Steps for SelectRxBand {
    type Part = Script;

    fn next_part(&mut self, failed: bool) -> Option<Script> {
        self.stage += 1;
        let (radio, band) = (&mut self.radio, self.band);
        match self.stage {
            1 => {
                radio.rx.band = Some(band);
                Some(radio.lms.select_lna(band.lna()))
            }
            2 => Some(
                Script::new()
                    .fpga(FPGA, Gpo::RxSwitch.write(band.rx_switch()))
                    .fpga(FPGA, Gpo::RxMixerEnable.write(u8::from(band.mixer()))),
            ),
            3 if band.mixer() != radio.rx.mixer_on => {
                radio.rx.mixer_on = band.mixer();
                // Switches output 3, the mixer LO, on or off with its output-enable bit alone, while
                // the chip stays ACTIVE.
                //
                // Intentional divergence from libusdr's `si5332_set_port3_en`, which requests READY to
                // change the power-down registers; in READY the chip drives no outputs, so the sample
                // clocks stop. Skyworks' Si5332 Reference Manual (rev 0.6, "Si5332 32-QFN Specific
                // Registers", p. 52) lists `OUT3_OE` as writable in ACTIVE; the uSDR's six-output part
                // uses that map, where `OUT3_OE` is bit 6, as in libusdr.
                //
                // Output 3's divider and driver are powered from power-up (`ClockSequence::program`) on, and
                // each sample rate (`ClockSequence::layout`) configures them, so the enable is all that changes: off, they
                // stay powered but disabled, where libusdr powers them down. As in libusdr, output 3
                // runs from the first sample rate until the first band crossing, because power-up
                // enables every output, and is silent before any rate. Output 2, the TX clock, keeps
                // its power-up enable (`OutputEnables0to3::ALL`). libusdr rewrites it only here, at a
                // band crossing: on while a TX stream exists (`tx_run`), as this leaves it, and off
                // otherwise, which this does not copy: a crossing with no TX stream keeps the TX clock
                // running.
                let enables = OutputEnables0to3::ALL.with_output3(radio.rx.mixer_on);
                let access = RegisterAccess::write(wiring::CLOCK, enables.entry());
                (!failed).then(|| Script::new().i2c(si5332::NAME, access))
            }
            _ => None,
        }
    }

    fn policy<E>(&self, _: &SequenceFailure<E>) -> Policy {
        if self.stage == 2 {
            Policy::Continue
        } else {
            Policy::Stop
        }
    }
}

parts! {
    /// A tuning's part.
    #[derive(Clone, Copy, Debug)]
    pub(crate) enum TunePart {
        /// The chain's power.
        Power(Run<ChainPower>),
        /// The RX band.
        RxBand(Run<SelectRxBand>),
        /// A synthesizer tuning.
        Pll(PllTune),
        /// Writes and waits.
        Script(Script),
    }
}

/// Where a tuning is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    /// Not started.
    Start,
    /// Powering the chain, any failure discarded.
    Power,
    /// Routing the band, any failure discarded.
    Band,
    /// The first tuning.
    First,
    /// The TX quirk: the RX PLL tuned to the same LO, its result discarded.
    RxFirst,
    /// The wait before the second tuning.
    Retry,
    /// The second tuning.
    Second,
    /// Sweeping up for the lowest LO that locks.
    Sweep,
    /// Tuning the lowest LO found before.
    Minimal,
    /// The NCOs.
    Ncos,
    /// The filter.
    Bandwidth,
    /// Finished.
    Done,
}

/// Tunes a chain to a frequency with its frequency plan: `usdr_rfic_fe_set_freq` once the
/// channels' LOs and offsets are recorded.
///
/// The band follows the frequency unless the caller fixed it. On RX, in the mixer band the
/// PLL tunes the frequency plus the mixer LO. As in libusdr, the first TX tune that cannot
/// lock, with no RX frequency set, tunes the RX PLL to the same LO before retrying ("some
/// LMS6002 chips fail to lock TX pll before RX initialization"), and the low-LO fallback is
/// the receiver's, on the chain's PLL and NCOs.
///
/// Divergences, where libusdr would divide by zero before any sample rate is set: the NCO
/// lock fallback fails as unlocked, and the filter is left alone. A third, RX only and
/// reachable only with the mixer band fixed: where the frequency plus the mixer LO passes
/// `u32::MAX`, libusdr converts the double sum to `unsigned`, which C leaves undefined (x86
/// wraps it), and tunes that; this fails as an unsupported frequency before tuning. On
/// failure, state is left as libusdr leaves it: power and band changes, LO offset and
/// minimal LO recorded at the same points.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tune {
    /// The state.
    pub(crate) radio: Radio,
    /// Which chain.
    chain: Chain,
    /// The frequency asked, in Hz.
    hz: u32,
    /// The LO the PLL tunes, in Hz.
    lo_hz: u32,
    /// TX: the chain's first tuning.
    first: bool,
    /// The sweep's LO.
    sweep_hz: u32,
    /// The last tuning's outcome, until acted on.
    tuned: Option<Result<CapacitorWindow, TuneFault>>,
    /// Where it is.
    stage: Stage,
    /// The fault that ended it.
    fault: Option<TuneFault>,
}

impl Tune {
    /// Tunes `chain` to `hz`.
    pub(crate) const fn new(radio: Radio, chain: Chain, hz: u32) -> Self {
        Self {
            radio,
            chain,
            hz,
            lo_hz: hz,
            first: false,
            sweep_hz: 0,
            tuned: None,
            stage: Stage::Start,
            fault: None,
        }
    }

    /// Tunes `channels` of `chain` to `hz` and the PLL to the average of the channels' LOs
    /// (`usdr_rfic_fe_set_freq` with `FE_FREQ_LO_RX` or `_TX` and a channel mask).
    pub(crate) fn channel_frequency(
        mut radio: Radio,
        chain: Chain,
        channels: NcoChannels,
        hz: u32,
    ) -> Self {
        let lo = match chain {
            Chain::Rx => radio.rx.plan.set_lo(channels, hz),
            Chain::Tx => radio.tx.plan.set_lo(channels, hz),
        };
        Self::new(radio, chain, lo)
    }

    /// Sets the baseband offset of `channels` of `chain` to `hz`, then tunes the chain again
    /// to its LO (`FE_FREQ_BB_RX` or `_TX`).
    pub(crate) fn baseband_offset(
        mut radio: Radio,
        chain: Chain,
        channels: NcoChannels,
        hz: i32,
    ) -> Self {
        let lo = match chain {
            Chain::Rx => {
                radio.rx.plan.set_baseband(channels, hz);
                radio.rx.lo_hz
            }
            Chain::Tx => {
                radio.tx.plan.set_baseband(channels, hz);
                radio.tx.lo_hz
            }
        };
        Self::new(radio, chain, lo)
    }

    /// Ends the tuning with `fault`.
    fn fail(&mut self, fault: TuneFault) -> Option<TunePart> {
        self.fault = Some(fault);
        self.stage = Stage::Done;
        None
    }

    /// The chain's lowest LO found, in Hz.
    fn minimal_lo_hz(&mut self) -> &mut u32 {
        match self.chain {
            Chain::Rx => &mut self.radio.rx.minimal_lo_hz,
            Chain::Tx => &mut self.radio.tx.minimal_lo_hz,
        }
    }

    /// The first tuning's PLL part, after the power and band: the LO for the frequency.
    fn first_tuning(&mut self) -> Option<TunePart> {
        let radio = &mut self.radio;
        match self.chain {
            Chain::Rx => {
                radio.rx.lo_offset_hz = 0;
                if radio.rx.mixer_on {
                    let Some(lo) = self.hz.checked_add(radio.mixer_lo_hz) else {
                        return self.fail(TuneFault::UnsupportedFrequency(self.hz));
                    };
                    self.lo_hz = lo;
                }
                self.radio.rx.pll_lo_hz = self.lo_hz;
            }
            Chain::Tx => radio.tx.lo_offset_hz = 0,
        }
        self.stage = Stage::First;
        Some(TunePart::Pll(self.radio.tune_pll(self.chain, self.lo_hz)))
    }

    /// libusdr's fallback for an LO below 250 MHz the PLL cannot lock at: tune the lowest
    /// LO that locks, found once by stepping up 1 MHz at a time and kept as the chain's
    /// minimal LO (0 until found), and offset the NCOs by the gap.
    fn extend(&mut self) -> Option<TunePart> {
        let lo = self.lo_hz;
        if lo >= LOW_LO_HZ {
            return self.fail(TuneFault::Unlocked(lo));
        }
        if *self.minimal_lo_hz() == 0 {
            self.sweep_hz = lo;
            self.stage = Stage::Sweep;
            return Some(TunePart::Pll(self.radio.tune_pll(self.chain, lo)));
        }
        self.stage = Stage::Minimal;
        let minimal = *self.minimal_lo_hz();
        Some(TunePart::Pll(self.radio.tune_pll(self.chain, minimal)))
    }

    /// The NCO offset for the gap from the minimal LO, provided the converter rate leaves
    /// room for it; then the NCOs. Both chains use the ADC rate and RX decimation: libusdr
    /// sets its DAC rate and TX interpolation to the same values on every rate change.
    fn offset(&mut self) -> Option<TunePart> {
        let lo = self.lo_hz;
        let minimal = *self.minimal_lo_hz();
        let radio = &self.radio;
        let Some(decimation) = radio.rx.decimation else {
            return self.fail(TuneFault::Unlocked(lo));
        };
        let offset = i64::from(lo) - i64::from(minimal);
        let baseband = radio.adc_rate_hz / decimation.factor();
        #[expect(
            clippy::cast_possible_truncation,
            reason = "libusdr adds the 64-bit gap into a 32-bit unsigned"
        )]
        let needed = baseband.wrapping_add(offset.unsigned_abs() as u32);
        if f64::from(radio.adc_rate_hz) * NCO_HEADROOM < f64::from(needed) {
            return self.fail(TuneFault::Unlocked(lo));
        }
        #[expect(
            clippy::cast_possible_truncation,
            reason = "libusdr stores the offset in a 32-bit int"
        )]
        let offset = offset as i32;
        match self.chain {
            Chain::Rx => self.radio.rx.lo_offset_hz = offset,
            Chain::Tx => self.radio.tx.lo_offset_hz = offset,
        }
        Some(self.ncos())
    }

    /// The part after a tuning ended with `outcome`.
    fn after_tuning(&mut self, outcome: Result<CapacitorWindow, TuneFault>) -> Option<TunePart> {
        match (self.stage, outcome) {
            (Stage::First, Err(TuneFault::Unlocked(_))) => {
                if self.chain == Chain::Tx && self.first && self.radio.rx.lo_hz == 0 {
                    // libusdr discards this tune's result too.
                    self.stage = Stage::RxFirst;
                    Some(TunePart::Pll(self.radio.tune_pll(Chain::Rx, self.hz)))
                } else {
                    // libusdr: the LDO may not be ready yet.
                    self.stage = Stage::Retry;
                    Some(TunePart::Script(
                        Script::new().sleep(Duration::from_millis(5)),
                    ))
                }
            }
            (Stage::Second, Err(TuneFault::Unlocked(_))) => self.extend(),
            (Stage::Sweep, Err(TuneFault::Unlocked(_))) => self.step_sweep(false),
            // A window this narrow at the bottom is not trusted; keep stepping.
            (Stage::Sweep, Ok(window)) if window.high < 2 => self.step_sweep(true),
            (Stage::Sweep, Ok(_)) => {
                *self.minimal_lo_hz() = self.sweep_hz;
                self.offset()
            }
            (Stage::Minimal, Ok(_)) => self.offset(),
            (_, Err(fault)) => self.fail(fault),
            (_, Ok(_)) => Some(self.ncos()),
        }
    }

    /// The sweep's next LO, 1 MHz up, while below 250 MHz; past it, the sweep ends with its
    /// last result: `locked` (a narrow window) keeps the LO reached, else it fails, naming
    /// the caller's LO rather than the sweep's last.
    fn step_sweep(&mut self, locked: bool) -> Option<TunePart> {
        self.sweep_hz += SWEEP_STEP_HZ;
        if self.sweep_hz < LOW_LO_HZ {
            return Some(TunePart::Pll(
                self.radio.tune_pll(self.chain, self.sweep_hz),
            ));
        }
        if !locked {
            return self.fail(TuneFault::Unlocked(self.lo_hz));
        }
        *self.minimal_lo_hz() = self.sweep_hz;
        self.offset()
    }

    /// The NCOs at the chain's offsets.
    fn ncos(&mut self) -> TunePart {
        self.stage = Stage::Ncos;
        TunePart::Script(self.radio.restore_ncos(self.chain))
    }
}

impl Steps for Tune {
    type Part = TunePart;

    fn next_part(&mut self, _failed: bool) -> Option<TunePart> {
        let (radio, chain) = (&mut self.radio, self.chain);
        match self.stage {
            Stage::Start => {
                match chain {
                    Chain::Rx => radio.rx.lo_hz = self.hz,
                    Chain::Tx => {
                        self.first = radio.tx.lo_hz == 0;
                        radio.tx.lo_hz = self.hz;
                    }
                }
                // `_usdr_signal_event(USDR_*_LO_CHANGED)`: power the chain, then the band for
                // the frequency, if it is not fixed. libusdr discards both results.
                self.stage = Stage::Power;
                Some(TunePart::Power(Run::new(ChainPower::new(
                    *radio, chain, true,
                ))))
            }
            Stage::Power => {
                self.stage = Stage::Band;
                match chain {
                    Chain::Rx if !radio.rx.band_fixed => {
                        let band = RxBand::for_frequency(self.hz);
                        Some(TunePart::RxBand(Run::new(SelectRxBand::new(*radio, band))))
                    }
                    Chain::Tx if !radio.tx.band_fixed => {
                        let band = TxBand::for_frequency(self.hz);
                        Some(TunePart::Script(radio.select_tx_band(band)))
                    }
                    Chain::Rx | Chain::Tx => self.first_tuning(),
                }
            }
            Stage::Band => self.first_tuning(),
            Stage::RxFirst => {
                self.tuned = None;
                self.stage = Stage::Retry;
                Some(TunePart::Script(
                    Script::new().sleep(Duration::from_millis(5)),
                ))
            }
            Stage::Retry => {
                self.stage = Stage::Second;
                Some(TunePart::Pll(self.radio.tune_pll(chain, self.lo_hz)))
            }
            Stage::First | Stage::Second | Stage::Sweep | Stage::Minimal => {
                // A bus failure stops the tuning before this.
                let outcome = self.tuned.take()?;
                self.after_tuning(outcome)
            }
            Stage::Ncos => {
                self.stage = Stage::Bandwidth;
                self.radio.update_bandwidth(chain).map(TunePart::Script)
            }
            Stage::Bandwidth | Stage::Done => None,
        }
    }

    fn absorb(&mut self, part: &TunePart) {
        match part {
            TunePart::Power(run) => self.radio = run.steps().radio,
            TunePart::RxBand(run) => self.radio = run.steps().radio,
            TunePart::Script(_) => {}
            TunePart::Pll(tune) => {
                self.radio.lms = *tune.chip();
                self.tuned = tune.outcome();
            }
        }
    }

    fn policy<E>(&self, _: &SequenceFailure<E>) -> Policy {
        match self.stage {
            Stage::Power | Stage::Band | Stage::RxFirst => Policy::Ignore,
            _ => Policy::Stop,
        }
    }

    fn fault(&self) -> Option<TuneFault> {
        self.fault
    }
}

owns_radio!(SelectRxBand, Tune);
