//! Board power: `pe_sync_power_on` and `pe_sync_power_off` in `device/pe_sync/pe_sync.c`.
//!
//! Both are GPO writes followed by LED fades (`leds_fader`); at power-on the fades double as
//! the supplies' settling time ("Wait for all LDOs to settle").

use core::time::Duration;

use rsl_lowlevel::{Sequence, SequenceAction, SequenceFailure};

use super::fpga::{
    self, DISTRIBUTION_ON, GENERATOR_ON, Gpo, INPUT_BUFFERS, INPUT_GPS_LDO, INPUT_GPS_PPS,
    LEDS_ALL, LEDS_GREEN, LEDS_OFF, LEDS_RED, SYNTH_ENABLE, SYNTH_LDO,
};
use super::outcome::outcome;

/// An LED fade (`leds_fader(dev, delay_us, from_mask, to_mask)`): over `duration_us`, in
/// 100 equal periods, the LEDs show `to` for a growing share of each period and `from` for
/// the rest, then stay at `to`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Fade {
    /// The whole fade, in microseconds (`delay_us`).
    duration_us: u32,
    /// The LEDs it starts from (`from_mask`).
    from: u8,
    /// The LEDs it ends at (`to_mask`).
    to: u8,
}

/// The fade's periods (`iter_count`).
const FADE_PERIODS: u32 = 100;

/// `pe_sync_power_on`'s writes: the GPS and reference inputs ("Enable GPS"), both LMX2820s,
/// the generator bank and the distribution supplies.
const POWER_ON_WRITES: [(Gpo, u8); 5] = [
    (Gpo::Input, INPUT_BUFFERS | INPUT_GPS_PPS | INPUT_GPS_LDO),
    (Gpo::Synth0, SYNTH_LDO | SYNTH_ENABLE),
    (Gpo::Synth1, SYNTH_LDO | SYNTH_ENABLE),
    (Gpo::Generator, GENERATOR_ON),
    (Gpo::Distribution, DISTRIBUTION_ON),
];

/// `pe_sync_power_on`'s fades, 1.1 s in all: dark to red, red to all, all to green.
const POWER_ON_FADES: [Fade; 3] = [
    Fade {
        duration_us: 100_000,
        from: LEDS_OFF,
        to: LEDS_RED,
    },
    Fade {
        duration_us: 500_000,
        from: LEDS_RED,
        to: LEDS_ALL,
    },
    Fade {
        duration_us: 500_000,
        from: LEDS_ALL,
        to: LEDS_GREEN,
    },
];

/// `pe_sync_power_off`'s writes: LEDs, then every supply bank, the inputs last.
const POWER_OFF_WRITES: [(Gpo, u8); 6] = [
    (Gpo::Leds, 0),
    (Gpo::Distribution, 0),
    (Gpo::Generator, 0),
    (Gpo::Synth0, 0),
    (Gpo::Synth1, 0),
    (Gpo::Input, 0),
];

/// `pe_sync_power_off`'s fade: green to dark over 1 s.
const POWER_OFF_FADES: [Fade; 1] = [Fade {
    duration_us: 1_000_000,
    from: LEDS_GREEN,
    to: LEDS_OFF,
}];

/// Where a fade is within one period, or at its end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FadeStep {
    /// Showing `to`.
    To,
    /// Holding `to` for the period's lit share.
    ToWait,
    /// Showing `from`.
    From,
    /// Holding `from` for the rest of the period.
    FromWait,
    /// Leaving the LEDs at `to`.
    Last,
}

/// A fade in progress.
#[derive(Clone, Copy, Debug)]
struct Fading {
    /// The fade.
    fade: Fade,
    /// The period, 0 to 99.
    period: u32,
    /// The step within it.
    step: FadeStep,
    /// The lit share of this period: libusdr's `brightness`, a `float` that grows by
    /// `1.0f / 100` each period, so its rounding is kept.
    brightness: f32,
}

impl Fading {
    /// The fade's start.
    const fn new(fade: Fade) -> Self {
        Self {
            fade,
            period: 0,
            step: FadeStep::To,
            brightness: 0.0,
        }
    }

    /// One period, in microseconds (`full_len`).
    const fn period_us(&self) -> u32 {
        self.fade.duration_us / FADE_PERIODS
    }

    /// The period's lit share, in microseconds: libusdr's `(unsigned)(brightness *
    /// full_len)`, a `float` product truncated.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "C's float arithmetic: full_len is at most 10 000, exact in f32; the product is non-negative"
    )]
    fn lit_us(&self) -> u32 {
        (self.brightness * self.period_us() as f32) as u32
    }

    /// The action this step takes.
    fn action(&self) -> SequenceAction {
        let wait = |us| SequenceAction::Sleep(Duration::from_micros(u64::from(us)));
        let leds = |data| SequenceAction::Fpga(Gpo::Leds.write(data));
        match self.step {
            FadeStep::To | FadeStep::Last => leds(self.fade.to),
            FadeStep::ToWait => wait(self.lit_us()),
            FadeStep::From => leds(self.fade.from),
            FadeStep::FromWait => wait(self.period_us() - self.lit_us()),
        }
    }

    /// Moves past this step; `false` once the last write is done.
    fn advance(&mut self) -> bool {
        self.step = match self.step {
            FadeStep::To => FadeStep::ToWait,
            FadeStep::ToWait => FadeStep::From,
            FadeStep::From => FadeStep::FromWait,
            FadeStep::FromWait => {
                self.period += 1;
                self.brightness += 1.0 / 100.0;
                if self.period < FADE_PERIODS {
                    FadeStep::To
                } else {
                    FadeStep::Last
                }
            }
            FadeStep::Last => return false,
        };
        true
    }
}

/// Where a power sequence is.
#[derive(Clone, Copy, Debug)]
enum Stage {
    /// Writing GPO bank `n` of the plan.
    Write(usize),
    /// Fading the LEDs, fade `n` of the plan.
    Fade(usize, Fading),
    /// Finished or failed.
    Done,
}

/// GPO writes, then LED fades, each part ending the sequence at its first failure (the
/// C's `res = res ? res : ...` chain). Inside a fade, as in `leds_fader`, only the final
/// write's failure counts: the per-period writes' results are not checked.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Power {
    /// The GPO writes, in order.
    writes: &'static [(Gpo, u8)],
    /// The fades after them, in order.
    fades: &'static [Fade],
    /// Where the sequence is.
    stage: Stage,
}

impl Power {
    /// `pe_sync_power_on`: inputs, synthesizers, generator and distribution supplies on,
    /// then 1.1 s of LED fades.
    pub(crate) const fn on() -> Self {
        Self::new(&POWER_ON_WRITES, &POWER_ON_FADES)
    }

    /// `pe_sync_power_off`: LEDs and every supply bank off, then a 1 s fade to dark.
    pub(crate) const fn off() -> Self {
        Self::new(&POWER_OFF_WRITES, &POWER_OFF_FADES)
    }

    /// The sequence from its first write.
    const fn new(writes: &'static [(Gpo, u8)], fades: &'static [Fade]) -> Self {
        Self {
            writes,
            fades,
            stage: Stage::Write(0),
        }
    }

    /// The stage after this part: the next write, the first fade, or the next fade.
    fn after(&self, stage: Stage) -> Stage {
        let fade = |index: usize| {
            self.fades
                .get(index)
                .map_or(Stage::Done, |&fade| Stage::Fade(index, Fading::new(fade)))
        };
        match stage {
            Stage::Write(index) if index + 1 < self.writes.len() => Stage::Write(index + 1),
            Stage::Write(_) => fade(0),
            Stage::Fade(index, _) => fade(index + 1),
            Stage::Done => Stage::Done,
        }
    }
}

impl Sequence for Power {
    fn next(&self) -> Option<SequenceAction> {
        match self.stage {
            Stage::Write(index) => {
                let (bank, data) = self.writes[index];
                Some(SequenceAction::Fpga(bank.write(data)))
            }
            Stage::Fade(_, fading) => Some(fading.action()),
            Stage::Done => None,
        }
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(action) = self.next() else {
            return Ok(());
        };
        match &mut self.stage {
            Stage::Fade(_, fading) if fading.step != FadeStep::Last => {
                fading.advance();
                return Ok(());
            }
            _ => {}
        }
        if let Err(failure) = outcome(action, result, fpga::NAME) {
            self.stage = Stage::Done;
            return Err(failure);
        }
        self.stage = self.after(self.stage);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::vec::Vec;

    use rsl_lowlevel::{Access, FpgaAccess};

    use super::*;

    /// What a sequence did: GPO writes as `(bank, data)` and sleeps in microseconds.
    #[derive(Debug, PartialEq, Eq)]
    enum Done {
        Gpo(u32, u32),
        Sleep(u128),
    }

    /// Runs `power` to its end, failing the action at `failing`.
    fn run(power: &mut Power, failing: Option<usize>) -> (Vec<Done>, Result<(), ()>) {
        let mut done = Vec::new();
        while let Some(action) = power.next() {
            let at = done.len();
            done.push(match action {
                SequenceAction::Fpga(FpgaAccess::Write { reg: 0, value }) => {
                    Done::Gpo(value >> 24, value & 0xff)
                }
                SequenceAction::Sleep(wait) => Done::Sleep(wait.as_micros()),
                other => panic!("power sequences only write GPOs and wait: {other:?}"),
            });
            let result = if failing == Some(at) { Err(()) } else { Ok(0) };
            if power.complete(result).is_err() {
                return (done, Err(()));
            }
        }
        (done, Ok(()))
    }

    #[test]
    fn power_on_enables_every_supply_before_the_fades_that_let_them_settle() {
        let (done, outcome) = run(&mut Power::on(), None);
        assert_eq!(outcome, Ok(()));
        assert_eq!(
            done[..5],
            [
                Done::Gpo(0, 0x0d),
                Done::Gpo(1, 0x03),
                Done::Gpo(2, 0x03),
                Done::Gpo(3, 0x3f),
                Done::Gpo(4, 0x1f),
            ]
        );
        assert_eq!(done.len(), 5 + 3 * (FADE_PERIODS as usize * 4 + 1));
        let waited: u128 = done
            .iter()
            .map(|done| match done {
                Done::Sleep(us) => *us,
                Done::Gpo(..) => 0,
            })
            .sum();
        assert_eq!(waited, 1_100_000, "every period is waited in full");
        assert_eq!(done.last(), Some(&Done::Gpo(6, u32::from(LEDS_GREEN))));
    }

    #[test]
    fn a_fades_lit_share_grows_with_libusdrs_float_rounding() {
        let (done, _) = run(&mut Power::off(), None);
        // After six writes, period n of a 1 s fade from green holds the dark LEDs for
        // (unsigned)(brightness * 10000) µs: five float steps of 0.01 sum to just under
        // 0.05, so period 5 holds 499, not 500 (the oracle's golden close shows the same).
        let lit: Vec<u128> = done[6..]
            .chunks(4)
            .take(6)
            .map(|period| match period[1] {
                Done::Sleep(us) => us,
                Done::Gpo(..) => panic!("a period waits second"),
            })
            .collect();
        assert_eq!(lit, [0, 100, 200, 300, 400, 499]);
    }

    #[test]
    fn a_failed_supply_write_ends_the_power_up() {
        let (done, outcome) = run(&mut Power::on(), Some(2));
        assert_eq!(outcome, Err(()));
        assert_eq!(done.len(), 3, "nothing after the failed write");
        let mut power = Power::on();
        power.complete::<()>(Ok(0)).unwrap();
        let failure = power.complete(Err(())).unwrap_err();
        assert_eq!(failure.access(), Some((fpga::NAME, Access::Write, 0)));
    }

    #[test]
    fn only_a_fades_final_write_can_fail_it_as_in_leds_fader() {
        let (done, outcome) = run(&mut Power::off(), Some(6));
        assert_eq!(outcome, Ok(()), "a period's write is not checked");
        assert_eq!(done.len(), 6 + FADE_PERIODS as usize * 4 + 1);
        let last = done.len() - 1;
        assert_eq!(run(&mut Power::off(), Some(last)).1, Err(()));
    }
}
