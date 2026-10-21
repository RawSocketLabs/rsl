//! The sample rate: `dev_m2_d09_4_ad45_2_rate_set` in `device/m2_lsdr/m2_da09_4_ad45_2.c`,
//! behind `/dm/rate/master` (and `/dm/rate/rxtxadcdac`, which passes its RX rate on).
//!
//! libusdr picks the largest decimation that keeps the ADC under 260 MS/s ([`RatePlan`]),
//! tunes the Si549 to the ADC rate and loads the FIR gearbox ([`RatePrelude`]); on a board
//! where open found an LMK04832 it lays the clock generator out for the output rate; then it
//! waits 100 ms, reads the clock generator's status and lock, resets the bitslip logic,
//! puts both ADCs in normal mode, routes the DSP's mixer, sets the LVDS PHY's clock delay
//! and pulses the DSP chain's reset ([`RateFinish`]). The clock generator's plan is computed
//! between the two, where libusdr computes it, so a rate it cannot plan fails after the
//! gearbox is loaded, as in libusdr.

use core::time::Duration;

use hal::{FpgaAccess, Sequence, SequenceAction, SequenceFailure};
use sdr_gearbox::{DecimationOnly, Factor, UCODE_BYTES};

use super::ads42lbx9;
use super::fpga::{self, ALL_CHANNELS, CLOCK_STATUS, FIFO_RESET, Gpo, PHY_CLOCK_LOAD};
use super::lmk04832::{self, ClockPlan, LAYOUT_WRITES};
use super::outcome::outcome;
use super::si549::SetFrequency;
use super::wiring::Wiring;

/// The ADC rate libusdr keeps under (`MAX_RATE`, 260 MS/s), in S/s.
const MAX_ADC_RATE: u64 = 260_000_000;

/// The wait for the clocks to settle (`usleep(100000)`, "TODO wait stble frequency").
const SETTLE: Duration = Duration::from_millis(100);

/// The DSP chain's reset pulse (`usleep(10)`).
const DSP_RESET_PULSE: Duration = Duration::from_micros(10);

/// The receive chain's decimation (`rx_decims`), with the FIR microcode libusdr loads for
/// it on the `LSDr`'s 7-series FPGA (`fgearbox_load_fir` with `DSP_7SERIES`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Decimation {
    /// ×2.
    X2,
    /// ×3.
    X3,
    /// ×4.
    X4,
    /// ×5.
    X5,
    /// ×6.
    X6,
    /// ×8.
    X8,
    /// ×16.
    X16,
    /// ×32.
    X32,
    /// ×64.
    X64,
    /// ×128.
    X128,
}

impl Decimation {
    /// libusdr's factors, in its order (`rx_decims`).
    const ALL: [Self; 10] = [
        Self::X2,
        Self::X3,
        Self::X4,
        Self::X5,
        Self::X6,
        Self::X8,
        Self::X16,
        Self::X32,
        Self::X64,
        Self::X128,
    ];

    /// The factor as a number.
    pub(crate) const fn factor(self) -> u32 {
        match self {
            Self::X2 => 2,
            Self::X3 => 3,
            Self::X4 => 4,
            Self::X5 => 5,
            Self::X6 => 6,
            Self::X8 => 8,
            Self::X16 => 16,
            Self::X32 => 32,
            Self::X64 => 64,
            Self::X128 => 128,
        }
    }

    /// The gearbox microcode for this factor.
    const fn table(self) -> &'static [u8; UCODE_BYTES] {
        match self {
            Self::X2 => Factor::X2.decimation_ucode(),
            Self::X3 => DecimationOnly::X3.decimation_ucode(),
            Self::X4 => Factor::X4.decimation_ucode(),
            Self::X5 => DecimationOnly::X5.decimation_ucode(),
            Self::X6 => DecimationOnly::X6.decimation_ucode(),
            Self::X8 => Factor::X8.decimation_ucode(),
            Self::X16 => Factor::X16.decimation_ucode(),
            Self::X32 => Factor::X32.decimation_ucode(),
            Self::X64 => DecimationOnly::X64.decimation_ucode(),
            Self::X128 => DecimationOnly::X128.decimation_ucode(),
        }
    }
}

/// What libusdr derives from a requested sample rate before touching the board.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RatePlan {
    /// The sample rate asked for, in S/s (`rxbb_rate`).
    pub(crate) rate: u32,

    /// The decimation (`rxbb_decim`): the largest of libusdr's factors, scanned in order,
    /// whose ADC rate stays under 260 MS/s; ×2 when none does.
    pub(crate) decimation: Decimation,

    /// The ADC's rate, `rate × decimation` truncated to 32 bits as libusdr stores it
    /// (`adc_rate`), in S/s.
    pub(crate) adc_rate: u32,
}

impl RatePlan {
    /// libusdr's plan for `rate`.
    pub(crate) fn new(rate: u32) -> Self {
        let decimation = Decimation::ALL
            .into_iter()
            .take_while(|decimation| {
                u64::from(rate) * u64::from(decimation.factor()) < MAX_ADC_RATE
            })
            .last()
            .unwrap_or(Decimation::X2);
        let [a, b, c, d, ..] = (u64::from(rate) * u64::from(decimation.factor())).to_le_bytes();
        Self {
            rate,
            decimation,
            adc_rate: u32::from_le_bytes([a, b, c, d]),
        }
    }

    /// The LVDS PHY's clock-delay tap libusdr picks for the rate: 2 from 320 MS/s, 3 from
    /// 300, 4 from 290, 5 from 275, 0 from 160, 6 from 110 and 0 below.
    const fn phy_tap(self) -> u8 {
        match self.rate {
            320_000_000.. => 2,
            300_000_000.. => 3,
            290_000_000.. => 4,
            275_000_000.. => 5,
            160_000_000.. => 0,
            110_000_000.. => 6,
            _ => 0,
        }
    }
}

/// The rate's first part: the Si549 tuned ([`SetFrequency`]), every ADC channel selected,
/// and the gearbox microcode loaded a byte per write with no wait (`fgearbox_load_fir`).
/// Every failure ends it, as libusdr returns on each.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RatePrelude {
    /// The oscillator's sequence, run first.
    oscillator: SetFrequency,

    /// The microcode.
    table: &'static [u8; 1280],

    /// Writes completed after the oscillator's: the channel mask, then the microcode.
    done: usize,

    /// An action failed; nothing more is issued.
    failed: bool,
}

impl RatePrelude {
    /// Starts the prelude of `plan`, with `oscillator` programming its ADC rate; nothing
    /// has been accessed yet.
    pub(crate) const fn new(plan: RatePlan, oscillator: SetFrequency) -> Self {
        Self {
            oscillator,
            table: plan.decimation.table(),
            done: 0,
            failed: false,
        }
    }

    /// The write after the oscillator's at `step`.
    fn write(&self, step: usize) -> Option<FpgaAccess> {
        if step == 0 {
            return Some(Gpo::ChannelMask.write(ALL_CHANNELS));
        }
        self.table
            .get(step - 1)
            .map(|&byte| Gpo::FirProgram.write(byte))
    }
}

impl Sequence for RatePrelude {
    fn next(&self) -> Option<SequenceAction> {
        if self.failed {
            return None;
        }
        self.oscillator
            .next()
            .or_else(|| self.write(self.done).map(SequenceAction::Fpga))
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(action) = self.next() else {
            return Ok(());
        };
        if self.oscillator.next().is_some() {
            return self
                .oscillator
                .complete(result)
                .inspect_err(|_| self.failed = true);
        }
        outcome(action, result, fpga::NAME).inspect_err(|_| self.failed = true)?;
        self.done += 1;
        Ok(())
    }
}

/// The rate's writes after the settling wait, in libusdr's `res ? res : ...` chain: the
/// bitslip logic's reset (the FIFO resets pulsed), both ADCs in normal mode (the first with
/// channel A on, the second with neither), every channel selected, the second mixer to 3,
/// the PHY's clock delay, the DSP reset raised; then its wait and the reset released.
const TAIL: usize = 2 + 2 * 5 + 6;

/// Where the tail's wait is in it.
const PULSE_WAIT: usize = TAIL - 2;

/// The rate's second part: on a board with an LMK04832, its layout and both ADC ports;
/// the settling wait; with the LMK04832, its status word and lock read twice (all three
/// only logged by libusdr); then the tail. A failure before the tail ends it; one in the
/// tail skips the writes left but still takes the reset pulse's wait, as libusdr's chain
/// does, and a failed release ends it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RateFinish {
    /// Where the chips answer.
    wiring: Wiring,

    /// The clock generator's writes, on a board with one.
    layout: Option<[(u16, u8); LAYOUT_WRITES]>,

    /// The rate.
    plan: RatePlan,

    /// Actions completed, or skipped after a failure in the tail.
    done: usize,

    /// An action failed.
    failed: bool,
}

impl RateFinish {
    /// Starts the second part of `plan` on `wiring`, laying `clock` out first when the board
    /// has an LMK04832; nothing has been accessed yet.
    pub(crate) fn new(wiring: Wiring, plan: RatePlan, clock: Option<ClockPlan>) -> Self {
        Self {
            wiring,
            layout: clock.map(ClockPlan::layout),
            plan,
            done: 0,
            failed: false,
        }
    }

    /// Where the tail starts.
    const fn tail_start(&self) -> usize {
        match self.layout {
            Some(_) => LAYOUT_WRITES + 1 + 3,
            None => 1,
        }
    }

    /// Where the tail's wait is.
    const fn pulse_wait(&self) -> usize {
        self.tail_start() + PULSE_WAIT
    }

    /// Action `step`, and the chip it reaches.
    fn action(&self, step: usize) -> Option<(SequenceAction, &'static str)> {
        let Wiring {
            adcs: [adc0, adc1],
            clock_generator,
            ..
        } = self.wiring;
        let fpga = |access| Some((SequenceAction::Fpga(access), fpga::NAME));
        let gpo = |bank: Gpo, data| fpga(bank.write(data));
        let clock = |access| Some((SequenceAction::Fpga(access), lmk04832::NAME));
        let wait = |duration| Some((SequenceAction::Sleep(duration), fpga::NAME));
        let layout = self.layout.as_ref().map_or(&[][..], |writes| &writes[..]);
        if let Some(&entry) = layout.get(step) {
            return clock(lmk04832::write_at(clock_generator, entry));
        }
        let step = step - layout.len();
        if step == 0 {
            return wait(SETTLE);
        }
        let step = step - 1;
        let checks = if self.layout.is_some() { 3 } else { 0 };
        if step < checks {
            return match step {
                0 => fpga(CLOCK_STATUS),
                _ => clock(lmk04832::lock_read(clock_generator)),
            };
        }
        let adc = |target, at: usize, any_channel| {
            let entry = ads42lbx9::normal_mode(any_channel)[at];
            Some((
                SequenceAction::Fpga(ads42lbx9::write_entry(target, entry)),
                ads42lbx9::NAME,
            ))
        };
        match step - checks {
            0 => gpo(Gpo::AdcConfig, FIFO_RESET),
            1 => gpo(Gpo::AdcConfig, 0),
            at @ 2..=6 => adc(adc0, at - 2, true),
            at @ 7..=11 => adc(adc1, at - 7, false),
            12 => gpo(Gpo::ChannelMask, ALL_CHANNELS),
            13 => gpo(Gpo::DspMix, 3),
            14 => gpo(Gpo::PhyControl, PHY_CLOCK_LOAD | self.plan.phy_tap()),
            15 => gpo(Gpo::DspReset, 1),
            16 => wait(DSP_RESET_PULSE),
            17 => gpo(Gpo::DspReset, 0),
            _ => None,
        }
    }

    /// The pending action and its chip.
    fn pending(&self) -> Option<(SequenceAction, &'static str)> {
        if self.failed && self.done != self.pulse_wait() {
            return None;
        }
        self.action(self.done)
    }
}

impl Sequence for RateFinish {
    fn next(&self) -> Option<SequenceAction> {
        self.pending().map(|(action, _)| action)
    }

    fn attempts_after_failure(&self) -> bool {
        true
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some((action, chip)) = self.pending() else {
            return Ok(());
        };
        let failure = outcome(action, result, chip).err();
        self.done += 1;
        let Some(failure) = failure else {
            return Ok(());
        };
        self.failed = true;
        if (self.tail_start()..self.pulse_wait()).contains(&(self.done - 1)) {
            self.done = self.pulse_wait();
        }
        Err(failure)
    }
}

#[cfg(test)]
mod tests {
    use hal::{I2cAddr, SpiAddr};

    use super::*;
    use crate::portable::si549::FrequencyPlan;

    /// The `LSDr`'s addresses.
    const WIRING: Wiring = Wiring {
        pmic: I2cAddr::new(0, 0x48),
        oscillator: I2cAddr::new(0, 0x67),
        adcs: [SpiAddr(0), SpiAddr(1)],
        clock_generator: SpiAddr(2),
    };

    /// How many leading actions [`run`] keeps.
    const KEPT: usize = 64;

    /// Runs `sequence` to its end, failing the action at `failing`: how many actions it
    /// issued, and the first [`KEPT`].
    fn run(
        sequence: &mut impl Sequence,
        failing: Option<usize>,
    ) -> (usize, [Option<SequenceAction>; KEPT]) {
        let mut kept = [None; KEPT];
        let mut count = 0;
        while let Some(action) = sequence.next() {
            let result = if failing == Some(count) {
                Err(())
            } else {
                Ok(0x31)
            };
            if let Some(slot) = kept.get_mut(count) {
                *slot = Some(action);
            }
            count += 1;
            if sequence.complete(result).is_err() && !sequence.attempts_after_failure() {
                break;
            }
        }
        (count, kept)
    }

    #[test]
    fn the_decimation_is_the_largest_that_keeps_the_adc_under_260_msps() {
        let plan = |rate| {
            let plan = RatePlan::new(rate);
            (plan.decimation.factor(), plan.adc_rate)
        };
        assert_eq!(plan(50_000_000), (5, 250_000_000));
        assert_eq!(plan(1_000_000), (128, 128_000_000));
        assert_eq!(plan(2_000_000), (128, 256_000_000));
        assert_eq!(plan(2_100_000), (64, 134_400_000));
        assert_eq!(plan(200_000_000), (2, 400_000_000), "none fits: x2");
    }

    #[test]
    fn the_prelude_tunes_the_oscillator_then_loads_every_microcode_byte() {
        let plan = RatePlan::new(50_000_000);
        let frequency = FrequencyPlan::new(plan.adc_rate).expect("in range");
        let mut prelude = RatePrelude::new(plan, SetFrequency::new(WIRING.oscillator, frequency));
        let (count, actions) = run(&mut prelude, None);
        assert_eq!(count, 14 + 1 + 1280);
        assert_eq!(
            actions[14],
            Some(SequenceAction::Fpga(FpgaAccess::Write {
                reg: 0,
                value: 0x0500_000f
            }))
        );
        assert_eq!(
            actions[15],
            Some(SequenceAction::Fpga(FpgaAccess::Write {
                reg: 0,
                value: 0x0f00_0000 | u32::from(DecimationOnly::X5.decimation_ucode()[0])
            }))
        );
    }

    #[test]
    fn without_a_clock_generator_the_finish_waits_then_runs_the_tail() {
        let mut finish = RateFinish::new(WIRING, RatePlan::new(50_000_000), None);
        let (count, actions) = run(&mut finish, None);
        assert_eq!(count, 1 + TAIL);
        assert_eq!(actions[0], Some(SequenceAction::Sleep(SETTLE)));
        assert_eq!(
            actions[1 + 14],
            Some(SequenceAction::Fpga(FpgaAccess::Write {
                reg: 0,
                value: 0x0600_0080
            })),
            "50 MS/s takes tap 0"
        );
    }

    #[test]
    fn a_clock_generator_is_laid_out_before_the_wait_and_checked_after() {
        let clock = ClockPlan::new(50_000_000, 50_000_000).expect("a plan");
        let mut finish = RateFinish::new(WIRING, RatePlan::new(50_000_000), Some(clock));
        let (count, actions) = run(&mut finish, None);
        assert_eq!(count, LAYOUT_WRITES + 1 + 3 + TAIL);
        assert_eq!(actions[LAYOUT_WRITES], Some(SequenceAction::Sleep(SETTLE)));
        assert_eq!(
            actions[LAYOUT_WRITES + 1],
            Some(SequenceAction::Fpga(CLOCK_STATUS))
        );
    }

    #[test]
    fn a_failure_in_the_tail_still_takes_the_reset_pulses_wait() {
        let mut finish = RateFinish::new(WIRING, RatePlan::new(50_000_000), None);
        let (count, actions) = run(&mut finish, Some(3));
        assert_eq!(count, 4 + 1);
        assert_eq!(actions[4], Some(SequenceAction::Sleep(DSP_RESET_PULSE)));

        let mut finish = RateFinish::new(WIRING, RatePlan::new(50_000_000), None);
        assert_eq!(run(&mut finish, Some(0)).0, 1 + TAIL, "a wait cannot fail");
        let mut finish = RateFinish::new(WIRING, RatePlan::new(50_000_000), None);
        assert_eq!(run(&mut finish, Some(TAIL)).0, 1 + TAIL);
    }

    #[test]
    fn a_failure_before_the_tail_ends_the_rate() {
        let clock = ClockPlan::new(50_000_000, 50_000_000).expect("a plan");
        let mut finish = RateFinish::new(WIRING, RatePlan::new(50_000_000), Some(clock));
        assert_eq!(run(&mut finish, Some(3)).0, 4);
    }
}
