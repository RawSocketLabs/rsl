//! The power rails without I/O: the PMIC and boost-converter part of `m2_lm6_1` power-up
//! (`usdr_init` in `device/m2_lm6_1/usdr_ctrl.c`, with `hw/lp8758/lp8758.c` and
//! `hw/tps6381x/tps6381x.c`).
//!
//! [`PowerRails`] yields one single-byte I²C register access at a time and takes its
//! outcome back (a [`Sequence`]). The blocking driver and any other executor run the same machine, so the
//! sequence exists once. The board chooses the setpoints ([`RailPlan`]).

use super::address::I2cAddr;
use super::lp8758::{self, Buck, BuckControl, BuckVoltage, Config, Revision};
use super::register::{IndexedRegister, Register};
use super::sequence::{RegisterAccess, Sequence, SequenceAction, SequenceFailure};
use super::tps6381x::{self, Control, DeviceId, OutputVoltage, Vsel};

/// The board's choices for its rails.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RailPlan {
    /// Where the LP8758 answers.
    pub(crate) pmic: I2cAddr,
    /// The PMIC's chip-wide configuration.
    pub(crate) config: Config,
    /// Channel 1, the FPGA GPIO bank rail.
    pub(crate) gpio: BuckVoltage,
    /// Channel 3, the LMS6002D I/O rail.
    pub(crate) lms_io: BuckVoltage,
    /// Written to all four channels' controls.
    pub(crate) control: BuckControl,
    /// Where the `TPS6381x` answers.
    pub(crate) boost: I2cAddr,
    /// Run the boost converter in forced PWM, ramping in PWM too.
    pub(crate) boost_forced_pwm: bool,
    /// The boost output, whichever level its VSEL pin is at.
    pub(crate) boost_voltage: OutputVoltage,
}

/// Where the sequence is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    /// Reading the PMIC's device revision (libusdr reads it first).
    PmicDevice,
    /// Reading its OTP revision, with the device revision already read.
    PmicOtp {
        /// The device revision byte.
        device: u8,
    },
    /// Writing its configuration.
    PmicConfig,
    /// Setting channel 1.
    PmicGpio,
    /// Setting channel 3.
    PmicLmsIo,
    /// Writing this channel's control, channels in order.
    PmicControl(Buck),
    /// Reading the boost converter's device ID.
    BoostId,
    /// Writing its control.
    BoostControl,
    /// Setting its VSEL-low output.
    BoostLow,
    /// Setting its VSEL-high output.
    BoostHigh,
    /// The rails are up.
    Done,
    /// A step failed; nothing more is issued.
    Failed,
}

/// The PMIC checks and rail setpoints, then the boost converter's (`tps6381x_init`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct PowerRails {
    /// The board's setpoints.
    plan: RailPlan,
    /// The access [`Self::next`] returns.
    step: Step,
}

impl PowerRails {
    /// Starts the sequence; nothing has been accessed yet.
    pub(crate) const fn new(plan: RailPlan) -> Self {
        Self {
            plan,
            step: Step::PmicDevice,
        }
    }

    /// The access to perform next; `None` once the rails are up or a step failed.
    pub(crate) fn access(&self) -> Option<RegisterAccess> {
        let plan = &self.plan;
        let (pmic, boost) = (plan.pmic, plan.boost);
        Some(match self.step {
            Step::PmicDevice => RegisterAccess::read(pmic, lp8758::Reg::DeviceRevision),
            Step::PmicOtp { .. } => RegisterAccess::read(pmic, lp8758::Reg::OtpRevision),
            Step::PmicConfig => RegisterAccess::write(pmic, plan.config.entry()),
            Step::PmicGpio => RegisterAccess::write(pmic, plan.gpio.entry_at(Buck::B1)),
            Step::PmicLmsIo => RegisterAccess::write(pmic, plan.lms_io.entry_at(Buck::B3)),
            Step::PmicControl(buck) => RegisterAccess::write(pmic, plan.control.entry_at(buck)),
            Step::BoostId => RegisterAccess::read(boost, tps6381x::Reg::DeviceId),
            Step::BoostControl => {
                let control = Control::new()
                    .with_enabled(true)
                    .with_forced_pwm(plan.boost_forced_pwm)
                    .with_ramp_pwm(plan.boost_forced_pwm);
                RegisterAccess::write(boost, control.entry())
            }
            Step::BoostLow => RegisterAccess::write(boost, plan.boost_voltage.entry_at(Vsel::Low)),
            Step::BoostHigh => {
                RegisterAccess::write(boost, plan.boost_voltage.entry_at(Vsel::High))
            }
            Step::Done | Step::Failed => return None,
        })
    }

    /// Applies the outcome of [`Self::next`]'s access: the byte read, or any byte for a
    /// write. A failure ends the sequence. An outcome with no access pending is ignored,
    /// like a duplicate.
    pub(crate) fn complete<E>(&mut self, result: Result<u8, E>) -> Result<(), SequenceFailure<E>> {
        let Some(access) = self.access() else {
            return Ok(());
        };
        let chip = self.chip();
        let byte = result.map_err(|source| {
            self.step = Step::Failed;
            SequenceFailure::Transport {
                chip,
                access,
                source,
            }
        })?;
        self.step = match self.step {
            Step::PmicDevice => Step::PmicOtp { device: byte },
            Step::PmicOtp { device } => {
                let found = Revision::new(byte, device);
                self.check(found == lp8758::REVISION, chip, || {
                    (lp8758::REVISION.to_raw().into(), found.to_raw().into())
                })?;
                Step::PmicConfig
            }
            Step::PmicConfig => Step::PmicGpio,
            Step::PmicGpio => Step::PmicLmsIo,
            Step::PmicLmsIo => Step::PmicControl(Buck::B0),
            Step::PmicControl(buck) => buck.next().map_or(Step::BoostId, Step::PmicControl),
            Step::BoostId => {
                let found = DeviceId::from(byte);
                self.check(found == tps6381x::DEVICE_ID, chip, || {
                    (tps6381x::DEVICE_ID.to_raw().into(), found.to_raw().into())
                })?;
                Step::BoostControl
            }
            Step::BoostControl => Step::BoostLow,
            Step::BoostLow => Step::BoostHigh,
            Step::BoostHigh => Step::Done,
            // `next` returned an access, so neither is current.
            Step::Done | Step::Failed => self.step,
        };
        Ok(())
    }

    /// Ends the sequence with a chip-ID failure unless `matches`; `ids` gives (expected,
    /// found).
    fn check<E>(
        &mut self,
        matches: bool,
        chip: &'static str,
        ids: impl FnOnce() -> (u32, u32),
    ) -> Result<(), SequenceFailure<E>> {
        if matches {
            return Ok(());
        }
        self.step = Step::Failed;
        let (expected, found) = ids();
        Err(SequenceFailure::ChipId {
            chip,
            expected,
            found,
        })
    }

    /// The chip the current step addresses.
    fn chip(&self) -> &'static str {
        match self.step {
            Step::BoostId | Step::BoostControl | Step::BoostLow | Step::BoostHigh => tps6381x::NAME,
            _ => lp8758::NAME,
        }
    }
}

impl Sequence for PowerRails {
    fn next(&self) -> Option<SequenceAction> {
        self.access().map(SequenceAction::Access)
    }

    fn complete<E>(&mut self, result: Result<u8, E>) -> Result<(), SequenceFailure<E>> {
        PowerRails::complete(self, result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The uSDR's rail plan, as the board sets it.
    fn plan() -> RailPlan {
        RailPlan {
            pmic: I2cAddr::new(0, 0x60),
            config: Config::new().with_warn_at_105c(true),
            gpio: BuckVoltage::from_millivolts(1800),
            lms_io: BuckVoltage::from_millivolts(1800),
            control: BuckControl::new()
                .with_enabled(true)
                .with_discharge_when_off(true)
                .with_forced_pwm(true),
            boost: I2cAddr::new(0, 0x75),
            boost_forced_pwm: true,
            boost_voltage: OutputVoltage::from_millivolts(3450),
        }
    }

    /// The accesses a run made, in order, without allocating (the fixture is `no_std`).
    struct Log {
        /// Room for the whole 13-access sequence.
        accesses: [Option<RegisterAccess>; 16],
        /// How many were made.
        len: usize,
    }

    impl Log {
        /// The accesses made.
        fn iter(&self) -> impl Iterator<Item = RegisterAccess> + '_ {
            self.accesses[..self.len].iter().flatten().copied()
        }
    }

    /// Runs `rails` to its end, answering reads from `reads` in order; returns the accesses.
    fn run(rails: &mut PowerRails, reads: &[u8]) -> (Log, Result<(), SequenceFailure<()>>) {
        let mut reads = reads.iter().copied();
        let mut log = Log {
            accesses: [None; 16],
            len: 0,
        };
        while let Some(access) = rails.access() {
            log.accesses[log.len] = Some(access);
            log.len += 1;
            let byte = match access {
                RegisterAccess::Read { .. } => reads.next().expect("a scripted read byte"),
                RegisterAccess::Write { .. } => 0,
            };
            if let Err(failure) = rails.complete::<()>(Ok(byte)) {
                return (log, Err(failure));
            }
        }
        (log, Ok(()))
    }

    /// The device and OTP revision bytes, then the boost ID, of the parts libusdr accepts.
    const GOOD_IDS: [u8; 3] = [0x01, 0xe0, 0x04];

    #[test]
    fn a_wrong_pmic_revision_stops_before_any_write() {
        let mut rails = PowerRails::new(plan());
        let (log, outcome) = run(&mut rails, &[0x01, 0xe1]);
        assert_eq!(
            outcome,
            Err(SequenceFailure::ChipId {
                chip: lp8758::NAME,
                expected: 0xe001,
                found: 0xe101,
            })
        );
        assert!(
            log.iter()
                .all(|access| matches!(access, RegisterAccess::Read { .. })),
            "nothing is written to an unrecognised PMIC"
        );
        assert_eq!(
            rails.access(),
            None,
            "a failed sequence issues nothing more"
        );
    }

    #[test]
    fn a_wrong_boost_id_stops_before_enabling_it() {
        let mut rails = PowerRails::new(plan());
        let (log, outcome) = run(&mut rails, &[0x01, 0xe0, 0x00]);
        assert_eq!(
            outcome,
            Err(SequenceFailure::ChipId {
                chip: tps6381x::NAME,
                expected: 0x04,
                found: 0x00,
            })
        );
        assert_eq!(
            log.iter().last(),
            Some(RegisterAccess::Read {
                dev: I2cAddr::new(0, 0x75),
                reg: 0x03,
            }),
            "the boost converter's DEVID read is the last access"
        );
    }

    #[test]
    fn a_transport_failure_names_the_failed_write() {
        // Both revision reads succeed; the CONFIG write is pending when the bus fails.
        let mut rails = PowerRails::new(plan());
        rails.complete::<()>(Ok(0x01)).unwrap();
        rails.complete::<()>(Ok(0xe0)).unwrap();
        let failure = rails.complete(Err("bus down"));
        assert_eq!(
            failure,
            Err(SequenceFailure::Transport {
                chip: lp8758::NAME,
                access: RegisterAccess::Write {
                    dev: I2cAddr::new(0, 0x60),
                    reg: 0x17,
                    value: 0x08,
                },
                source: "bus down",
            })
        );
        assert_eq!(rails.access(), None);
    }

    #[test]
    fn a_failed_boost_write_names_the_boost_converter() {
        let mut rails = PowerRails::new(plan());
        // The PMIC steps and the boost DEVID read succeed; CONTROL is pending.
        while rails.access()
            != Some(RegisterAccess::Read {
                dev: I2cAddr::new(0, 0x75),
                reg: 0x03,
            })
        {
            let byte = match rails.access() {
                Some(RegisterAccess::Read { reg: 0x00, .. }) => 0x01,
                Some(RegisterAccess::Read { reg: 0x01, .. }) => 0xe0,
                _ => 0,
            };
            rails.complete::<()>(Ok(byte)).unwrap();
        }
        rails.complete::<()>(Ok(0x04)).unwrap();
        assert!(matches!(
            rails.complete(Err(())),
            Err(SequenceFailure::Transport {
                chip: tps6381x::NAME,
                access: RegisterAccess::Write { reg: 0x01, .. },
                ..
            })
        ));
    }

    #[test]
    fn every_channel_is_controlled_once_and_the_rails_finish() {
        let mut rails = PowerRails::new(plan());
        let (log, outcome) = run(&mut rails, &GOOD_IDS);
        assert_eq!(outcome, Ok(()));
        assert_eq!(rails.access(), None, "the rails are up");
        let controls = log.iter().filter_map(|access| match access {
            RegisterAccess::Write { dev, reg, .. }
                if dev == I2cAddr::new(0, 0x60) && [2, 4, 6, 8].contains(&reg) =>
            {
                Some(reg)
            }
            _ => None,
        });
        assert!(
            controls.eq([2, 4, 6, 8]),
            "BUCK0..3_CTRL1, once each, in order"
        );
        assert_eq!(
            rails.complete::<()>(Ok(0)),
            Ok(()),
            "a late outcome is ignored"
        );
        assert_eq!(rails.access(), None);
    }
}
