//! The LMX2820's operations after a tune, as `hw/lmx2820/lmx2820.c` runs them:
//! `lmx2820_tune_instcal` (a retune to new outputs over the same phase detector, with
//! instant calibration instead of a full one) and `lmx2820_sync` (a phase-sync pulse). No
//! libusdr device calls them; `PE_SYNC` has no control for them (`PORTING.md`).

use core::time::Duration;

use hal::{Sequence, SequenceAction, SequenceFailure, SpiAddr};

use super::plan::Plan;
use super::reg::Reg;
use super::sequence::NAME;
use crate::portable::outcome::outcome;
use crate::portable::script::Script;
use crate::portable::ti_spi::{self, Modify};

/// The R0 bits an instant retune clears (`INSTCAL_R0_MASK`): `FCAL_EN`, `DBLR_CAL_EN` and
/// `INSTCAL_SKIP_ACAL`.
const INSTANT_R0_CLEAR: u16 = 1 << 4 | 1 << 6 | 1 << 13;

/// `PHASE_SYNC_EN`'s bit in R1.
const PHASE_SYNC: u16 = 1 << 15;

/// `lmx2820_tune_instcal`'s bus part, for a plan from [`Plan::retune`]: R0 read, then the
/// plan's outputs, instant numerator, fraction and dividers, R0 written back without its
/// calibration bits, and instant calibration's wait.
#[derive(Clone, Copy, Debug)]
pub(crate) struct InstantTune {
    /// The chip.
    target: SpiAddr,
    /// The writes before R0.
    registers: [(u8, u16); 10],
    /// Instant calibration's wait.
    delay: Duration,
    /// Everything after the R0 read, once it is done.
    rest: Option<Script>,
    /// The R0 read failed.
    failed: bool,
}

impl InstantTune {
    /// Starts retuning the chip at `target` to `plan`.
    pub(crate) fn new(target: SpiAddr, plan: &Plan) -> Self {
        Self {
            target,
            registers: plan.instant_registers(),
            delay: Duration::from_micros(plan.instant_delay_us().into()),
            rest: None,
            failed: false,
        }
    }

    /// The R0 read the last write starts from.
    const fn read_control(&self) -> SequenceAction {
        ti_spi::read(self.target, Reg::Control as u8)
    }
}

impl Sequence for InstantTune {
    fn next(&self) -> Option<SequenceAction> {
        if self.failed {
            return None;
        }
        match &self.rest {
            None => Some(self.read_control()),
            Some(rest) => rest.next(),
        }
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        if let Some(rest) = &mut self.rest {
            return rest.complete(result);
        }
        if self.failed {
            return Ok(());
        }
        let word =
            outcome(self.read_control(), result, NAME).inspect_err(|_| self.failed = true)?;
        let control = ti_spi::value(word) & !INSTANT_R0_CLEAR;
        let mut rest = Script::new();
        for entry in self.registers {
            rest = rest.then(NAME, ti_spi::write(self.target, entry));
        }
        self.rest = Some(
            rest.then(
                NAME,
                ti_spi::write(self.target, (Reg::Control as u8, control)),
            )
            .then(NAME, SequenceAction::Sleep(self.delay)),
        );
        Ok(())
    }
}

/// `lmx2820_sync`: R1 read, then written back with `PHASE_SYNC_EN` clear, set and clear.
pub(crate) const fn sync(target: SpiAddr) -> Modify {
    Modify::cleared_pulse(NAME, target, Reg::Sync as u8, PHASE_SYNC)
}

#[cfg(test)]
mod tests {
    use hal::Access;

    use super::super::plan::Request;
    use super::super::reg::MashOrder;
    use super::*;
    use crate::portable::testing::{Exchange, drive};

    /// The board's LMX2820 #0 bus.
    const TARGET: SpiAddr = SpiAddr(2);

    #[test]
    fn an_instant_retune_writes_r0_last_without_its_calibration_bits_then_waits() {
        let request = Request {
            reference_hz: 150_000_000,
            mash: MashOrder::Second,
            multiplier: None,
            out_a_hz: 0,
            out_b_hz: 0,
            sysref: None,
        };
        let setup = Plan::for_instant_calibration(&request).expect("a 300 MHz phase detector");
        let plan = setup
            .retune(1_000_000_000, 1_000_000_000)
            .expect("8 GHz over 8");
        let (done, outcome) = drive(&mut InstantTune::new(TARGET, &plan), |_| Ok(0x6670));
        assert_eq!(outcome, Ok(()));
        assert_eq!(
            done.first(),
            Some(&Exchange::Spi(0x0080_0000)),
            "R0 read first"
        );
        assert_eq!(done.len(), 1 + 10 + 2);
        assert_eq!(
            done[11..],
            [Exchange::Spi(0x0000_4620), Exchange::Sleep(798)],
            "FCAL_EN, DBLR_CAL_EN and INSTCAL_SKIP_ACAL cleared; 2.5 / 0.47 µs per MHz"
        );
    }

    #[test]
    fn a_failed_r0_read_ends_the_instant_retune_before_any_write() {
        let request = Request {
            reference_hz: 150_000_000,
            mash: MashOrder::Second,
            multiplier: None,
            out_a_hz: 1_000_000_000,
            out_b_hz: 1_000_000_000,
            sysref: None,
        };
        let plan = Plan::solve(&request).expect("libusdr's plan");
        let mut retune = InstantTune::new(TARGET, &plan);
        let failure = retune.complete(Err(())).unwrap_err();
        assert_eq!(failure.access(), Some((NAME, Access::Read, 0x00)));
        assert_eq!(retune.next(), None);
    }
}
