//! RFIC power-up on the `new_rev` boards (XSDR): `xsdr_pwren(.., true)` with
//! `_xsdr_pwren_revx` in `device/m2_lm7_1/xsdr_ctrl.c`, then `lms7002m_create`.
//!
//! The LMS7002M is held in reset while its external LDOs come on, then its own LDOs, then
//! the reset is released and the chip programmed (`rsl-lms7002m`'s [`Create`]). The waits
//! are libusdr's.

use core::time::Duration;

use rsl_lms7002m::{Create, Lms7002m, ResetPlan};
use rsl_lowlevel::{Sequence, SequenceAction, SequenceFailure};

use super::fpga::{self, Gpo, LMS_LDO_ENABLE, LMS_RESET_RELEASE};
use super::outcome::outcome;

/// Where the sequence is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    /// Holding the LMS7002M in reset with its LDOs off ("Disable, put into reset").
    Reset,
    /// libusdr's 5 ms after that.
    ResetWait,
    /// Enabling the external LDOs (`_xsdr_pwren_revx`).
    ExternalLdo,
    /// `_xsdr_pwren_revx`'s 1 ms.
    ExternalLdoWait,
    /// `xsdr_pwren`'s 5 ms after it.
    SupplyWait,
    /// Enabling the chip's LDOs, still in reset ("Enable LDO, put into reset").
    ChipLdo,
    /// 1 ms.
    ChipLdoWait,
    /// Releasing the reset ("Enable LDO, reset release").
    Release,
    /// 2.5 ms.
    ReleaseWait,
    /// Loading the reset programme and checking the version.
    Create,
}

/// `xsdr_pwren(.., true)` on an XSDR whose PMIC revision libusdr accepted.
///
/// libusdr refuses the power-up of a board whose PMIC revision it did not accept, after
/// holding the LMS7002M in reset; the driver refuses before running this sequence at all
/// (`PORTING.md`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct PowerUp {
    /// The step [`Sequence::next`] names.
    step: Step,

    /// The LMS7002M's part, run at [`Step::Create`].
    create: Create,

    /// A step failed; nothing more is issued.
    failed: bool,
}

impl PowerUp {
    /// Starts the sequence; nothing has been accessed yet.
    pub(crate) fn new(lms: ResetPlan) -> Self {
        Self {
            step: Step::Reset,
            create: Create::new(lms),
            failed: false,
        }
    }

    /// The LMS7002M as `lms7002m_create` left it, once the power-up succeeded.
    pub(crate) const fn chip(&self) -> Option<Lms7002m> {
        self.create.chip()
    }
}

impl Sequence for PowerUp {
    fn next(&self) -> Option<SequenceAction> {
        let wait = |us| Some(SequenceAction::Sleep(Duration::from_micros(us)));
        if self.failed {
            return None;
        }
        match self.step {
            Step::Reset => Some(SequenceAction::Fpga(Gpo::LmsPower.write(0))),
            Step::ResetWait | Step::SupplyWait => wait(5000),
            Step::ExternalLdo => Some(SequenceAction::Fpga(Gpo::LmsLdo.write(1))),
            Step::ExternalLdoWait | Step::ChipLdoWait => wait(1000),
            Step::ChipLdo => Some(SequenceAction::Fpga(Gpo::LmsPower.write(LMS_LDO_ENABLE))),
            Step::Release => Some(SequenceAction::Fpga(
                Gpo::LmsPower.write(LMS_LDO_ENABLE | LMS_RESET_RELEASE),
            )),
            Step::ReleaseWait => wait(2500),
            Step::Create => self.create.next(),
        }
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        if self.step == Step::Create {
            return self
                .create
                .complete(result)
                .inspect_err(|_| self.failed = true);
        }
        let Some(action) = self.next() else {
            return Ok(());
        };
        outcome(action, result, fpga::NAME).inspect_err(|_| self.failed = true)?;
        self.step = match self.step {
            Step::Reset => Step::ResetWait,
            Step::ResetWait => Step::ExternalLdo,
            Step::ExternalLdo => Step::ExternalLdoWait,
            Step::ExternalLdoWait => Step::SupplyWait,
            Step::SupplyWait => Step::ChipLdo,
            Step::ChipLdo => Step::ChipLdoWait,
            Step::ChipLdoWait => Step::Release,
            Step::Release => Step::ReleaseWait,
            Step::ReleaseWait | Step::Create => Step::Create,
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rsl_lowlevel::{FpgaAccess, SpiAddr};

    use super::*;

    /// The LMS7002M on SPI bus 0, nothing optional.
    const LMS: ResetPlan = ResetPlan {
        target: SpiAddr(0),
        ldo_mask: 0,
        shared_reference: false,
    };

    /// Runs `power` to its end, failing every wait and answering SPI reads with `version`.
    fn run(power: &mut PowerUp, version: u32) -> Result<(), SequenceFailure<()>> {
        while let Some(action) = power.next() {
            let result = match action {
                SequenceAction::Sleep(_) => Err(()),
                SequenceAction::Fpga(FpgaAccess::Spi { write: false, .. }) => Ok(version),
                _ => Ok(0),
            };
            power.complete(result)?;
        }
        Ok(())
    }

    #[test]
    fn a_wait_reported_as_failed_still_counts_as_waited() {
        let mut power = PowerUp::new(LMS);
        assert_eq!(run(&mut power, 0x3840), Ok(()));
        assert!(power.chip().is_some(), "the LMS7002M is created");
    }

    #[test]
    fn a_wrong_lms7002m_ends_the_power_up() {
        let mut power = PowerUp::new(LMS);
        assert!(matches!(
            run(&mut power, 0),
            Err(SequenceFailure::ChipId {
                chip: "LMS7002M",
                ..
            })
        ));
        assert_eq!(power.next(), None);
        assert_eq!(power.chip(), None, "a refused chip is not created");
    }
}
