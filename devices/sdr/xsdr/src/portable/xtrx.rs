//! The supply bring-up at open on the XTRX: `_xsdr_init_revo` in
//! `device/m2_lm7_1/xsdr_ctrl.c`, with `hw/lp8758/lp8758.c`, up to its reference-trim DAC.
//!
//! The LMS7002M is held in reset, the external GPIOs switched to 3.3 V and the second I2C
//! bus handed to the FPGA; the LMS-side LP8758 on that bus is identified, all four of its
//! rails set and enabled in forced PWM and its power-good status read once; the bus goes
//! back to GPIO control, and the FPGA-side LP8758 is identified, its I/O rail set and its
//! four converters enabled likewise. The board chooses the setpoints ([`XtrxSupplyPlan`]).
//! The trim DAC that follows takes 16-bit I2C transfers, which the portable seam does not
//! express, so the driver identifies and sets it with blocking code.

use hal::{I2cAddr, IndexedRegister, RegisterAccess, Sequence, SequenceAction, SequenceFailure};
use lp8758::{self, Buck, BuckControl, BuckVoltage, Revision};

use super::fpga::{Gpo, LMS_PULSE};
use super::outcome::outcome;

/// [`Gpo::LmsPower`] bit 4 on the XTRX: the second I2C bus to the FPGA's I2C core ("Take
/// control of second I2C bus"; cleared again by "Switch second I2C to gpio control").
const LMS_I2C_HANDOVER: u8 = 1 << 4;

/// [`Gpo::IoVoltage`]'s value at open: "Set external GPIOs to 3.3V".
const IO_3V3: u8 = 1;

/// The XTRX's choices for its two PMICs.
#[derive(Clone, Copy, Debug)]
pub(crate) struct XtrxSupplyPlan {
    /// The LMS-side LP8758 (`I2C_BUS_LP8758_LMSINIT`).
    pub(crate) lms_pmic: I2cAddr,

    /// Its four outputs, channel 0 first.
    pub(crate) lms_rails: [BuckVoltage; 4],

    /// The FPGA-side LP8758 (`I2C_BUS_LP8758_FPGA`).
    pub(crate) fpga_pmic: I2cAddr,

    /// The one rail set on it, and its output.
    pub(crate) fpga_rail: (Buck, BuckVoltage),

    /// Written to every channel's control on both PMICs, channel 0 first.
    pub(crate) control: BuckControl,
}

/// Which PMIC a step addresses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pmic {
    /// The LMS-side one, on the handed-over bus.
    Lms,
    /// The FPGA-side one.
    Fpga,
}

/// Where the sequence is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    /// The LMS7002M held in reset ([`LMS_PULSE`], as on the XSDR).
    Reset,
    /// The external GPIOs at 3.3 V.
    IoVoltage,
    /// The second I2C bus to the FPGA.
    Handover,
    /// Reading a PMIC's device revision.
    Device(Pmic),
    /// Reading its OTP revision, the device revision already read.
    Otp(Pmic, u8),
    /// Setting the LMS PMIC's rail on this channel.
    LmsRail(Buck),
    /// Writing this channel's control.
    Control(Pmic, Buck),
    /// Reading the LMS PMIC's channels 0 and 1 status.
    Status01,
    /// Channels 2 and 3.
    Status23,
    /// The second I2C bus back to GPIO control.
    Release,
    /// Setting the FPGA PMIC's rail.
    FpgaRail,
    /// The supplies are up.
    Done,
    /// A step failed; nothing more is issued.
    Failed,
}

/// `_xsdr_init_revo`'s supply part.
///
/// A PMIC whose revision is not libusdr's 0xE001 ends the open, as libusdr's `-EIO` does
/// (unlike the XSDR, whose open carries on). The power-good wait reads the status once:
/// libusdr's `lp8758_check_pg(.., 0xf, ..)` accepts every status, so its first poll ends the
/// wait, with no sleep before it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct XtrxSupplies {
    /// The board's setpoints.
    plan: XtrxSupplyPlan,

    /// The step [`Sequence::next`] names.
    step: Step,
}

impl XtrxSupplies {
    /// Starts the sequence; nothing has been accessed yet.
    pub(crate) const fn new(plan: XtrxSupplyPlan) -> Self {
        Self {
            plan,
            step: Step::Reset,
        }
    }

    /// Where `pmic` answers.
    const fn address(&self, pmic: Pmic) -> I2cAddr {
        match pmic {
            Pmic::Lms => self.plan.lms_pmic,
            Pmic::Fpga => self.plan.fpga_pmic,
        }
    }

    /// The step after the current one, given the byte it reported; a refused revision is
    /// the failure.
    fn advance<E>(&self, byte: u8) -> Result<Step, SequenceFailure<E>> {
        Ok(match self.step {
            Step::Reset => Step::IoVoltage,
            Step::IoVoltage => Step::Handover,
            Step::Handover => Step::Device(Pmic::Lms),
            Step::Device(pmic) => Step::Otp(pmic, byte),
            Step::Otp(pmic, device) => {
                let revision = Revision::new(byte, device);
                if revision != lp8758::REVISION {
                    return Err(SequenceFailure::ChipId {
                        chip: lp8758::NAME,
                        expected: lp8758::REVISION.to_raw().into(),
                        found: revision.to_raw().into(),
                    });
                }
                match pmic {
                    Pmic::Lms => Step::LmsRail(Buck::B0),
                    Pmic::Fpga => Step::FpgaRail,
                }
            }
            Step::LmsRail(buck) => buck
                .next()
                .map_or(Step::Control(Pmic::Lms, Buck::B0), Step::LmsRail),
            Step::Control(Pmic::Lms, buck) => buck
                .next()
                .map_or(Step::Status01, |next| Step::Control(Pmic::Lms, next)),
            Step::Status01 => Step::Status23,
            Step::Status23 => Step::Release,
            Step::Release => Step::Device(Pmic::Fpga),
            Step::FpgaRail => Step::Control(Pmic::Fpga, Buck::B0),
            Step::Control(Pmic::Fpga, buck) => buck
                .next()
                .map_or(Step::Done, |next| Step::Control(Pmic::Fpga, next)),
            Step::Done | Step::Failed => self.step,
        })
    }
}

/// A channel's index in the plan's arrays.
const fn channel(buck: Buck) -> usize {
    match buck {
        Buck::B0 => 0,
        Buck::B1 => 1,
        Buck::B2 => 2,
        Buck::B3 => 3,
    }
}

impl Sequence for XtrxSupplies {
    fn next(&self) -> Option<SequenceAction> {
        let write = |pmic, entry| SequenceAction::Access(RegisterAccess::write(pmic, entry));
        let read = |pmic, reg| SequenceAction::Access(RegisterAccess::read(pmic, reg));
        let lms = self.plan.lms_pmic;
        Some(match self.step {
            Step::Reset => SequenceAction::Fpga(Gpo::LmsPower.write(LMS_PULSE)),
            Step::IoVoltage => SequenceAction::Fpga(Gpo::IoVoltage.write(IO_3V3)),
            Step::Handover => SequenceAction::Fpga(Gpo::LmsPower.write(LMS_I2C_HANDOVER)),
            Step::Device(pmic) => read(self.address(pmic), lp8758::Reg::DeviceRevision),
            Step::Otp(pmic, _) => read(self.address(pmic), lp8758::Reg::OtpRevision),
            Step::LmsRail(buck) => write(lms, self.plan.lms_rails[channel(buck)].entry_at(buck)),
            Step::Control(pmic, buck) => {
                write(self.address(pmic), self.plan.control.entry_at(buck))
            }
            Step::Status01 => read(lms, lp8758::Reg::Buck01Status),
            Step::Status23 => read(lms, lp8758::Reg::Buck23Status),
            Step::Release => SequenceAction::Fpga(Gpo::LmsPower.write(0)),
            Step::FpgaRail => {
                let (buck, voltage) = self.plan.fpga_rail;
                write(self.plan.fpga_pmic, voltage.entry_at(buck))
            }
            Step::Done | Step::Failed => return None,
        })
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(action) = self.next() else {
            return Ok(());
        };
        let value =
            outcome(action, result, lp8758::NAME).inspect_err(|_| self.step = Step::Failed)?;
        self.step = self
            .advance(value.to_le_bytes()[0])
            .inspect_err(|_| self.step = Step::Failed)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use hal::Access;

    use super::*;

    /// A plan whose rails are easy to tell apart.
    const PLAN: XtrxSupplyPlan = XtrxSupplyPlan {
        lms_pmic: I2cAddr::new(1, 0x60),
        lms_rails: [
            BuckVoltage::from_millivolts(2060),
            BuckVoltage::from_millivolts(3360),
            BuckVoltage::from_millivolts(1760),
            BuckVoltage::from_millivolts(1500),
        ],
        fpga_pmic: I2cAddr::new(0, 0x60),
        fpga_rail: (Buck::B1, BuckVoltage::from_millivolts(3000)),
        control: BuckControl::new().with_enabled(true).with_forced_pwm(true),
    };

    /// Runs `supplies`, answering revision reads as an LP8758-E0 and every other read 0:
    /// the actions issued, or the failure.
    fn run(
        supplies: &mut XtrxSupplies,
        otp: impl Fn(I2cAddr) -> u8,
    ) -> Result<usize, SequenceFailure<()>> {
        let mut actions = 0;
        while let Some(action) = supplies.next() {
            actions += 1;
            let value = match action {
                SequenceAction::Access(RegisterAccess::Read { reg: 0x00, .. }) => 0x01,
                SequenceAction::Access(RegisterAccess::Read { dev, reg: 0x01 }) => otp(dev),
                _ => 0,
            };
            supplies.complete::<()>(Ok(value.into()))?;
        }
        Ok(actions)
    }

    #[test]
    fn both_pmics_are_brought_up_in_libusdrs_order() {
        let mut supplies = XtrxSupplies::new(PLAN);
        // 3 GPO writes, 2 + 4 + 4 + 2 on the LMS PMIC, the release, 2 + 1 + 4 on the FPGA's.
        assert_eq!(run(&mut supplies, |_| 0xe0), Ok(3 + 12 + 1 + 7));
        assert_eq!(supplies.next(), None);
    }

    #[test]
    fn a_wrong_revision_on_either_pmic_ends_the_open() {
        for refused in [PLAN.lms_pmic, PLAN.fpga_pmic] {
            let mut supplies = XtrxSupplies::new(PLAN);
            let failure = run(
                &mut supplies,
                |dev| if dev == refused { 0xe1 } else { 0xe0 },
            )
            .expect_err("refused");
            assert_eq!(
                failure,
                SequenceFailure::ChipId {
                    chip: "LP8758",
                    expected: 0xe001,
                    found: 0xe101,
                }
            );
            assert_eq!(supplies.next(), None, "nothing follows the refusal");
        }
    }

    #[test]
    fn a_failed_pmic_access_names_the_lp8758() {
        let mut supplies = XtrxSupplies::new(PLAN);
        for _ in 0..3 {
            supplies.complete::<()>(Ok(0)).expect("GPO writes");
        }
        let failure = supplies.complete(Err(())).expect_err("bus failure");
        assert_eq!(failure.access(), Some(("LP8758", Access::Read, 0x00)));
        assert_eq!(supplies.next(), None);
    }
}
