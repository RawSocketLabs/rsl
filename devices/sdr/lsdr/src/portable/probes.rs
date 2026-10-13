//! What is fitted: `usdr_device_m2_d09_4_ad45_2_initialize` from its LMK04832 check to its
//! second "FIXUP" block (`device/m2_lsdr/m2_da09_4_ad45_2.c`).
//!
//! libusdr reads no hardware ID on the `LSDr`. With the ADC supply on
//! ([`Supplies`](super::supplies::Supplies)), it tests for an LMK04832 when asked (`lmk=1`),
//! reads each ADC socket's `REG_HF0`, then switches the ADC supply off again. An absent chip
//! is a finding, not a failure (libusdr tolerates `-ENODEV`); a failed access ends open.

use rsl_lowlevel::{Register, RegisterAccess, Sequence, SequenceAction, SequenceFailure};

use super::ads42lbx9::{self, Reg as AdcReg};
use super::lmk04832::ClockGeneratorProbe;
use super::outcome::outcome;
use super::tps6594::{self, Ldo4Control};
use super::wiring::Wiring;

/// The chips the probes found.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Fitted {
    /// An `ADS42LBx9` answered on each ADC socket (SPI buses 0 and 1).
    pub adcs: [bool; 2],

    /// The LMK04832 test passed; only run when asked for at open.
    pub clock_generator: bool,
}

/// The probes; see the module docs.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Probes {
    /// Where the chips answer.
    wiring: Wiring,

    /// The LMK04832 test, when asked for.
    clock_generator: Option<ClockGeneratorProbe>,

    /// ADC reads and the supply write completed.
    done: usize,

    /// What was found so far.
    fitted: Fitted,

    /// An action failed; nothing more is issued.
    failed: bool,
}

impl Probes {
    /// Starts the probes; the LMK04832 is tested only with `clock_generator` (libusdr's
    /// `lmk=1`).
    pub(crate) fn new(wiring: Wiring, clock_generator: bool) -> Self {
        Self {
            wiring,
            clock_generator: clock_generator
                .then(|| ClockGeneratorProbe::new(wiring.clock_generator)),
            done: 0,
            fitted: Fitted::default(),
            failed: false,
        }
    }

    /// What the probes found, once the sequence finished.
    pub(crate) const fn fitted(&self) -> Fitted {
        self.fitted
    }

    /// The clock-generator test, while it has exchanges left.
    fn testing_clock_generator(&self) -> Option<&ClockGeneratorProbe> {
        self.clock_generator
            .as_ref()
            .filter(|probe| probe.next().is_some())
    }

    /// The ADC read or supply write at `step`, and the chip it reaches.
    fn action(&self, step: usize) -> Option<(SequenceAction, &'static str)> {
        match self.wiring.adcs.get(step) {
            Some(&adc) => Some((
                SequenceAction::Fpga(ads42lbx9::read(adc, AdcReg::Hf0)),
                ads42lbx9::NAME,
            )),
            None if step == self.wiring.adcs.len() => Some((
                SequenceAction::Access(RegisterAccess::write(
                    self.wiring.pmic,
                    Ldo4Control::switched(false).entry(),
                )),
                tps6594::NAME,
            )),
            None => None,
        }
    }
}

impl Sequence for Probes {
    fn next(&self) -> Option<SequenceAction> {
        if self.failed {
            return None;
        }
        self.testing_clock_generator()
            .and_then(Sequence::next)
            .or_else(|| self.action(self.done).map(|(action, _)| action))
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        if self.failed {
            return Ok(());
        }
        if let Some(probe) = self
            .clock_generator
            .as_mut()
            .filter(|probe| probe.next().is_some())
        {
            probe.complete(result).inspect_err(|_| self.failed = true)?;
            self.fitted.clock_generator = probe.present();
            return Ok(());
        }
        let Some((action, chip)) = self.action(self.done) else {
            return Ok(());
        };
        let word = outcome(action, result, chip).inspect_err(|_| self.failed = true)?;
        if let Some(found) = self.fitted.adcs.get_mut(self.done) {
            *found = ads42lbx9::is_present(word);
        }
        self.done += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rsl_lowlevel::{FpgaAccess, I2cAddr, SpiAddr};

    use super::*;

    /// The `LSDr`'s addresses.
    const WIRING: Wiring = Wiring {
        pmic: I2cAddr::new(0, 0x48),
        oscillator: I2cAddr::new(0, 0x67),
        adcs: [SpiAddr(0), SpiAddr(1)],
        clock_generator: SpiAddr(2),
    };

    /// Runs `probes`, answering an SPI exchange on bus `n` with `answers[n]`; returns the
    /// number of actions issued.
    fn run(probes: &mut Probes, answers: [u32; 3]) -> usize {
        let mut issued = 0;
        while let Some(action) = probes.next() {
            let answer = match action {
                SequenceAction::Fpga(FpgaAccess::Spi { target, .. }) => {
                    answers[usize::from(target.bus())]
                }
                _ => 0,
            };
            probes.complete::<()>(Ok(answer)).unwrap();
            issued += 1;
        }
        issued
    }

    #[test]
    fn without_lmk_1_the_clock_generator_is_left_alone() {
        let mut probes = Probes::new(WIRING, false);
        assert_eq!(
            run(&mut probes, [0x6c, u32::MAX, 0]),
            3,
            "two ADC reads, LDO4 off"
        );
        assert_eq!(
            probes.fitted(),
            Fitted {
                adcs: [true, false],
                clock_generator: false,
            }
        );
    }

    #[test]
    fn with_lmk_1_the_clock_generator_is_tested_first() {
        let mut probes = Probes::new(WIRING, true);
        assert!(matches!(
            probes.next(),
            Some(SequenceAction::Fpga(FpgaAccess::Spi {
                target: SpiAddr(2),
                ..
            }))
        ));
        assert_eq!(run(&mut probes, [0x6d, 0x6c, u32::MAX]), 7 + 3);
        assert_eq!(
            probes.fitted(),
            Fitted {
                adcs: [true, true],
                clock_generator: false,
            },
            "a floating readback is no LMK04832"
        );
    }

    #[test]
    fn a_failed_adc_read_ends_open_before_the_supply_is_cut() {
        let mut probes = Probes::new(WIRING, false);
        probes.complete::<()>(Ok(0x6c)).unwrap();
        let failure = probes.complete(Err(())).unwrap_err();
        assert_eq!(
            failure.access().map(|(chip, ..)| chip),
            Some(ads42lbx9::NAME)
        );
        assert_eq!(probes.next(), None, "as libusdr returns there");
    }
}
