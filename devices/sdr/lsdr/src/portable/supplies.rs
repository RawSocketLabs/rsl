//! The supply bring-up at open: `usdr_device_m2_d09_4_ad45_2_initialize` in
//! `device/m2_lsdr/m2_da09_4_ad45_2.c`, from its PMIC check to the end of its first "FIXUP"
//! block.
//!
//! The TPS6594's identification is read (libusdr logs it, checks nothing), four bucks are
//! set, the ADC supply (LDO4) is switched off, the Si549's output gated, and LDO4 switched
//! on again so the probes that follow ([`Probes`](super::probes::Probes)) can reach the ADCs.

use core::time::Duration;

use hal::{IndexedRegister, Register, RegisterAccess, Sequence, SequenceAction, SequenceFailure};

use super::outcome::outcome;
use super::si549::{self, OutputControl};
use super::tps6594::{self, Buck, BuckVoltage, IDENTIFICATION, Ldo4Control};
use super::wiring::Wiring;

/// The board's choices for its supplies.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SupplyPlan {
    /// Where the chips answer.
    pub(crate) wiring: Wiring,

    /// The bucks set, in order, and their outputs.
    pub(crate) rails: [(Buck, BuckVoltage); 4],
}

/// libusdr's waits around the oscillator gate and after LDO4 comes back (`usleep`).
const SETTLE: [Duration; 3] = [
    Duration::from_micros(1000),
    Duration::from_micros(1000),
    Duration::from_micros(2500),
];

/// The supply part of open; see the module docs. Every failure ends it, as libusdr returns
/// on each.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Supplies {
    /// The board's setpoints.
    plan: SupplyPlan,

    /// Actions completed.
    done: usize,

    /// An action failed; nothing more is issued.
    failed: bool,
}

impl Supplies {
    /// Starts the sequence; nothing has been accessed yet.
    pub(crate) const fn new(plan: SupplyPlan) -> Self {
        Self {
            plan,
            done: 0,
            failed: false,
        }
    }

    /// Action `step`, and the chip it reaches.
    fn action(&self, step: usize) -> Option<(SequenceAction, &'static str)> {
        let Wiring {
            pmic, oscillator, ..
        } = self.plan.wiring;
        let pmic_action = |access| Some((SequenceAction::Access(access), tps6594::NAME));
        let ldo4 = |on| {
            pmic_action(RegisterAccess::write(
                pmic,
                Ldo4Control::switched(on).entry(),
            ))
        };
        let wait = |at: usize| Some((SequenceAction::Sleep(SETTLE[at]), tps6594::NAME));
        let rails = IDENTIFICATION.len();
        let gate = rails + self.plan.rails.len();
        match step {
            _ if step < rails => pmic_action(RegisterAccess::read(pmic, IDENTIFICATION[step])),
            _ if step < gate => {
                let (buck, voltage) = self.plan.rails[step - rails];
                pmic_action(RegisterAccess::write(pmic, voltage.entry_at(buck)))
            }
            _ => match step - gate {
                0 => ldo4(false),
                1 => wait(0),
                2 => Some((
                    SequenceAction::Access(RegisterAccess::write(
                        oscillator,
                        OutputControl::new().with_enabled(false).entry(),
                    )),
                    si549::NAME,
                )),
                3 => wait(1),
                4 => ldo4(true),
                5 => wait(2),
                _ => None,
            },
        }
    }
}

impl Sequence for Supplies {
    fn next(&self) -> Option<SequenceAction> {
        if self.failed {
            return None;
        }
        self.action(self.done).map(|(action, _)| action)
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some((action, chip)) = self.action(self.done).filter(|_| !self.failed) else {
            return Ok(());
        };
        outcome(action, result, chip).inspect_err(|_| self.failed = true)?;
        self.done += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use hal::{Access, I2cAddr, SpiAddr};

    use super::*;

    /// The `LSDr`'s addresses with rails that are easy to tell apart.
    const PLAN: SupplyPlan = SupplyPlan {
        wiring: Wiring {
            pmic: I2cAddr::new(0, 0x48),
            oscillator: I2cAddr::new(0, 0x67),
            adcs: [SpiAddr(0), SpiAddr(1)],
            clock_generator: SpiAddr(2),
        },
        rails: [
            (Buck::B1, BuckVoltage::from_millivolts(700)),
            (Buck::B2, BuckVoltage::from_millivolts(800)),
            (Buck::B3, BuckVoltage::from_millivolts(900)),
            (Buck::B4, BuckVoltage::from_millivolts(1000)),
        ],
    };

    #[test]
    fn the_adcs_are_left_supplied_after_the_oscillator_is_gated() {
        let mut supplies = Supplies::new(PLAN);
        let mut writes = [(0, 0, 0); 7];
        let mut count = 0;
        while let Some(action) = supplies.next() {
            if let SequenceAction::Access(RegisterAccess::Write { dev, reg, value }) = action {
                writes[count] = (dev.addr(), reg, value);
                count += 1;
            }
            supplies.complete::<()>(Ok(0)).unwrap();
        }
        assert_eq!(count, 4 + 3, "four rails, LDO4 off, the gate, LDO4 on");
        assert_eq!(writes[4], (0x48, 0x20, 0x20), "LDO4 off first");
        assert_eq!(writes[5], (0x67, 17, 0), "then the Si549's output gated");
        assert_eq!(writes[6], (0x48, 0x20, 0x33), "and LDO4 on for the probes");
    }

    #[test]
    fn a_failed_oscillator_write_names_the_si549_and_stops() {
        let mut supplies = Supplies::new(PLAN);
        for _ in 0..3 + 4 + 2 {
            supplies.complete::<()>(Ok(0)).unwrap();
        }
        let failure = supplies.complete(Err(())).unwrap_err();
        assert_eq!(failure.access(), Some((si549::NAME, Access::Write, 17)));
        assert_eq!(supplies.next(), None);
    }

    #[test]
    fn a_reported_failure_of_a_wait_still_counts_as_waited() {
        let mut supplies = Supplies::new(PLAN);
        for _ in 0..=3 + 4 {
            supplies.complete::<()>(Ok(0)).unwrap();
        }
        assert!(matches!(supplies.next(), Some(SequenceAction::Sleep(_))));
        assert_eq!(supplies.complete(Err(())), Ok(()));
        assert!(supplies.next().is_some());
    }
}
