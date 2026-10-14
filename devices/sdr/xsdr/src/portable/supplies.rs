//! The supply bring-up at open on the `new_rev` boards (XSDR): `_xsdr_init_revx` in
//! `device/m2_lm7_1/xsdr_ctrl.c`, with `hw/lp8758/lp8758.c`, up to the front-end probe.
//!
//! The LMS7002M power control is pulsed, the LP8758's revision read, three rails set and
//! all four converters enabled, the power-good status read once, and the internal reference
//! selected. The board chooses the setpoints ([`SupplyPlan`]).

use core::time::Duration;

use lp8758::{self, Buck, BuckControl, BuckVoltage, Revision};
use rsl_lowlevel::{
    I2cAddr, IndexedRegister, RegisterAccess, Sequence, SequenceAction, SequenceFailure,
};

use super::fpga::{CLOCK_INTERNAL, Gpo, LMS_PULSE};
use super::outcome::outcome;

/// libusdr's wait before each power-good poll (`usleep(1000)`).
const SETTLE: Duration = Duration::from_micros(1000);

/// The board's choices for its rails.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SupplyPlan {
    /// Where the LP8758 answers.
    pub(crate) pmic: I2cAddr,

    /// The channels set, in order, and their outputs.
    pub(crate) rails: [(Buck, BuckVoltage); 3],

    /// Written to all four channels' controls, channel 0 first.
    pub(crate) control: BuckControl,
}

/// Where the sequence is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    /// Setting [`LMS_PULSE`] in the LMS7002M power control.
    PulseHigh,
    /// Clearing the power control again.
    PulseLow,
    /// Reading the PMIC's device revision (libusdr reads it first).
    PmicDevice,
    /// Reading its OTP revision, with the device revision already read.
    PmicOtp {
        /// The device revision byte.
        device: u8,
    },
    /// Setting the plan's rail at this index.
    Rail(usize),
    /// Writing this channel's control.
    Control(Buck),
    /// Waiting before the power-good poll.
    Settle,
    /// Reading channels 0 and 1's status.
    Status01,
    /// Reading channels 2 and 3's status.
    Status23,
    /// Selecting the internal reference clock.
    Clock,
    /// The supplies are up.
    Done,
    /// A step failed; nothing more is issued.
    Failed,
}

/// `_xsdr_init_revx`'s supply part for the XSDR.
///
/// A PMIC revision other than libusdr's 0xE001 does not stop it: libusdr logs it, carries
/// on, and refuses only the RFIC power-up later ([`Self::pmic_revision`] keeps it for that).
///
/// The power-good wait polls once. libusdr's `lp8758_check_pg(.., 0xf, ..)` accepts every
/// status (a channel mask derived from the status is always within 0xF), so its up to 100
/// polls end on the first that reads; the status bytes are read, as libusdr does, and not
/// judged. A real power-good check would diverge from libusdr (`PORTING.md`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Supplies {
    /// The board's setpoints.
    plan: SupplyPlan,

    /// The step [`Sequence::next`] names.
    step: Step,

    /// The PMIC's revision as read; zero until then.
    revision: Revision,
}

impl Supplies {
    /// Starts the sequence; nothing has been accessed yet.
    pub(crate) const fn new(plan: SupplyPlan) -> Self {
        Self {
            plan,
            step: Step::PulseHigh,
            revision: Revision::new(0, 0),
        }
    }

    /// The PMIC's revision, once the sequence finished.
    pub(crate) const fn pmic_revision(self) -> Revision {
        self.revision
    }

    /// The step after the current one, given the byte or word it reported.
    fn advance(&mut self, value: u32) -> Step {
        let byte = value.to_le_bytes()[0];
        match self.step {
            Step::PulseHigh => Step::PulseLow,
            Step::PulseLow => Step::PmicDevice,
            Step::PmicDevice => Step::PmicOtp { device: byte },
            Step::PmicOtp { device } => {
                self.revision = Revision::new(byte, device);
                Step::Rail(0)
            }
            Step::Rail(index) if index + 1 < self.plan.rails.len() => Step::Rail(index + 1),
            Step::Rail(_) => Step::Control(Buck::B0),
            Step::Control(buck) => buck.next().map_or(Step::Settle, Step::Control),
            Step::Settle => Step::Status01,
            Step::Status01 => Step::Status23,
            Step::Status23 => Step::Clock,
            Step::Clock => Step::Done,
            Step::Done | Step::Failed => self.step,
        }
    }
}

impl Sequence for Supplies {
    fn next(&self) -> Option<SequenceAction> {
        let pmic = self.plan.pmic;
        Some(match self.step {
            Step::PulseHigh => SequenceAction::Fpga(Gpo::LmsPower.write(LMS_PULSE)),
            Step::PulseLow => SequenceAction::Fpga(Gpo::LmsPower.write(0)),
            Step::PmicDevice => {
                SequenceAction::Access(RegisterAccess::read(pmic, lp8758::Reg::DeviceRevision))
            }
            Step::PmicOtp { .. } => {
                SequenceAction::Access(RegisterAccess::read(pmic, lp8758::Reg::OtpRevision))
            }
            Step::Rail(index) => {
                let (buck, voltage) = self.plan.rails[index];
                SequenceAction::Access(RegisterAccess::write(pmic, voltage.entry_at(buck)))
            }
            Step::Control(buck) => SequenceAction::Access(RegisterAccess::write(
                pmic,
                self.plan.control.entry_at(buck),
            )),
            Step::Settle => SequenceAction::Sleep(SETTLE),
            Step::Status01 => {
                SequenceAction::Access(RegisterAccess::read(pmic, lp8758::Reg::Buck01Status))
            }
            Step::Status23 => {
                SequenceAction::Access(RegisterAccess::read(pmic, lp8758::Reg::Buck23Status))
            }
            Step::Clock => SequenceAction::Fpga(Gpo::ClockConfig.write(CLOCK_INTERNAL)),
            Step::Done | Step::Failed => return None,
        })
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(action) = self.next() else {
            return Ok(());
        };
        let value =
            outcome(action, result, lp8758::NAME).inspect_err(|_| self.step = Step::Failed)?;
        self.step = self.advance(value);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rsl_lowlevel::{Access, FpgaAccess};

    use super::*;

    /// A plan whose rails are easy to tell apart.
    const PLAN: SupplyPlan = SupplyPlan {
        pmic: I2cAddr::new(0, 0x60),
        rails: [
            (Buck::B1, BuckVoltage::from_millivolts(1500)),
            (Buck::B2, BuckVoltage::from_millivolts(1600)),
            (Buck::B3, BuckVoltage::from_millivolts(1700)),
        ],
        control: BuckControl::new().with_enabled(true),
    };

    /// Runs `supplies` to its end, answering every read with `byte`.
    fn run(supplies: &mut Supplies, byte: u8) {
        while supplies.next().is_some() {
            supplies.complete::<()>(Ok(byte.into())).unwrap();
        }
    }

    #[test]
    fn any_power_good_status_ends_the_wait_like_libusdrs_vacuous_check() {
        for status in [0x00, 0x44, 0xff] {
            let mut supplies = Supplies::new(PLAN);
            run(&mut supplies, status);
            assert_eq!(
                supplies.next(),
                None,
                "status {status:#04x} ends the sequence"
            );
        }
    }

    #[test]
    fn a_wrong_pmic_revision_is_kept_for_power_up_not_refused_at_open() {
        let mut supplies = Supplies::new(PLAN);
        run(&mut supplies, 0xe1);
        assert_eq!(supplies.next(), None, "every rail is still set");
        assert_eq!(supplies.pmic_revision(), Revision::new(0xe1, 0xe1));
    }

    #[test]
    fn a_failed_pmic_read_names_the_lp8758_register_and_stops() {
        let mut supplies = Supplies::new(PLAN);
        supplies.complete::<()>(Ok(0)).unwrap();
        supplies.complete::<()>(Ok(0)).unwrap();
        let failure = supplies.complete(Err(())).unwrap_err();
        assert_eq!(failure.access(), Some((lp8758::NAME, Access::Read, 0x00)));
        assert_eq!(supplies.next(), None);
    }

    #[test]
    fn a_failed_gpo_write_names_the_fpga_register() {
        let mut supplies = Supplies::new(PLAN);
        let failure = supplies.complete(Err(())).unwrap_err();
        assert!(matches!(
            failure,
            SequenceFailure::FpgaTransport {
                chip: "FPGA",
                access: FpgaAccess::Write { reg: 0, .. },
                ..
            }
        ));
    }
}
