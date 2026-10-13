//! The LMX1204's bus operations as `hw/lmx1204/lmx1204.c` runs them: `lmx1204_create`, the
//! register part of `lmx1204_solver`, `lmx1204_read_status` and `lmx1204_wait_pll_lock`.

use core::time::Duration;

use rsl_lowlevel::{Sequence, SequenceAction, SequenceFailure, SpiAddr};

use super::plan::Plan;
use super::reg::{LockStatus, MuxOut, Reg, Temperature};
use crate::portable::chain::{Lenient, Then};
use crate::portable::poll::{LockPoll, Wait};
use crate::portable::script::Script;
use crate::portable::ti_spi::{self, Register};

/// The chip's name in errors.
pub(crate) const NAME: &str = "LMX1204";

/// The pause after `lmx1204_create`'s writes, before the temperature read.
const CREATE_SETTLE: Duration = Duration::from_millis(1);

/// The pause after the solver's reset (`lmx1204_reset`).
const RESET_SETTLE: Duration = Duration::from_millis(10);

/// The lock wait's poll interval (`usleep(1000)` in `lmx1204_wait_pll_lock`).
const LOCK_INTERVAL: Duration = Duration::from_millis(1);

/// The lock wait `PE_SYNC` allows (`lmx1204_wait_pll_lock(.., 1000000)`).
const LOCK_TIMEOUT: Duration = Duration::from_secs(1);

/// `lmx1204_read_status`: the temperature, lock and VCO readbacks, which libusdr only
/// logs; the first failed read ends it.
pub(crate) const fn status(target: SpiAddr) -> Script {
    Script::new()
        .then(NAME, ti_spi::read(target, Reg::Temperature as u8))
        .then(NAME, ti_spi::read(target, Reg::LockStatus as u8))
        .then(NAME, ti_spi::read(target, Reg::VcoSelect as u8))
}

/// `lmx1204_create`: MUXOUT and the temperature sensor set up, 1 ms, the temperature read
/// (whose failure fails the create), then a status read whose failure does not.
pub(crate) type Create = Then<Script, Lenient<Script>>;

/// Starts `lmx1204_create` on the chip at `target`.
pub(crate) fn create(target: SpiAddr) -> Create {
    let write = |entry| ti_spi::write(target, entry);
    let setup = Script::new()
        .then(NAME, write((Reg::MuxOutOverride.into(), 0)))
        .then(NAME, write(Temperature::new().with_counting(true).entry()))
        .then(NAME, write(MuxOut::readback(true).entry()))
        .then(NAME, SequenceAction::Sleep(CREATE_SETTLE))
        .then(NAME, ti_spi::read(target, Reg::Temperature as u8));
    Then::new(setup, Lenient(status(target)))
}

/// `lmx1204_solver`'s bus part: a soft reset (R0 = 1, then 0), 10 ms, then the plan's
/// registers, the last of which starts the multiplier's calibration.
pub(crate) fn configure(target: SpiAddr, plan: &Plan) -> Script {
    let control = |value| ti_spi::write(target, (Reg::Control.into(), value));
    let mut script = Script::new()
        .then(NAME, control(1))
        .then(NAME, control(0))
        .then(NAME, SequenceAction::Sleep(RESET_SETTLE));
    for entry in plan.registers() {
        script = script.then(NAME, ti_spi::write(target, entry));
    }
    script
}

/// `lmx1204_wait_pll_lock`: nothing outside multiplier mode; otherwise R75 polled every
/// millisecond until the multiplier reports lock, then the state machine clock switched off
/// "to reduce spurs" (that write's failure is ignored, as libusdr ignores it). A timed-out
/// wait ends without failure; [`WaitLock::locked`] says so.
#[derive(Clone, Copy, Debug)]
pub(crate) struct WaitLock {
    /// The plan uses the multiplier, so there is a lock to wait for.
    multiplies: bool,
    /// The wait.
    poll: LockPoll,
    /// The state machine clock's switch-off, once locked.
    switch_off: Lenient<Script>,
}

impl WaitLock {
    /// Starts the wait for the chip at `target`, configured for `plan`.
    pub(crate) fn new(target: SpiAddr, plan: &Plan) -> Self {
        Self {
            multiplies: plan.multiplies(),
            poll: LockPoll::new(
                NAME,
                ti_spi::read(target, Reg::LockStatus as u8),
                |value| LockStatus::from_raw(value).locked(),
                LOCK_INTERVAL,
                LOCK_TIMEOUT,
            ),
            switch_off: Lenient(
                Script::new().then(NAME, ti_spi::write(target, plan.state_machine_off())),
            ),
        }
    }

    /// Whether the multiplier locked, or there was none to wait for.
    pub(crate) fn locked(&self) -> bool {
        !self.multiplies || self.poll.outcome() == Wait::Locked
    }
}

impl Sequence for WaitLock {
    fn next(&self) -> Option<SequenceAction> {
        if !self.multiplies {
            return None;
        }
        match self.poll.outcome() {
            Wait::Pending => self.poll.next(),
            Wait::Locked => self.switch_off.next(),
            Wait::TimedOut | Wait::Failed => None,
        }
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        if !self.multiplies {
            return Ok(());
        }
        match self.poll.outcome() {
            Wait::Pending => self.poll.complete(result),
            Wait::Locked => self.switch_off.complete(result),
            Wait::TimedOut | Wait::Failed => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use rsl_lowlevel::FpgaAccess;

    use super::super::plan::{Channel, Request};
    use super::super::reg::{Format, SysrefModeCode};
    use super::*;
    use crate::portable::testing::{Exchange, drive};

    /// The board's LMX1204 bus.
    const TARGET: SpiAddr = SpiAddr(4);

    /// `PE_SYNC`'s LMX1204 plan.
    fn plan() -> Plan {
        let all = Channel {
            enabled: true,
            clock: true,
            sysref: true,
        };
        Plan::solve(&Request {
            clkin_hz: 1_000_000_000,
            sysref_request_hz: 25_000_000,
            filter: false,
            sysref_mode: SysrefModeCode::Continuous,
            sysref: true,
            channels: [all; 5],
            clkout_hz: 4_000_000_000,
            sysref_out_hz: 3_125_000,
            logic_hz: 125_000_000,
            logic_clock_format: Format::Lvds,
            logic_sysref_format: Format::Lvds,
        })
        .expect("libusdr's plan")
    }

    /// Answers R75 with `lock` in its lock-detect bits, everything else with 0.
    fn with_lock(lock: u32) -> impl FnMut(&SequenceAction) -> Result<u32, ()> {
        move |action| {
            Ok(match action {
                SequenceAction::Fpga(FpgaAccess::Spi {
                    word: 0x00cb_0000, ..
                }) => lock << 8,
                _ => 0,
            })
        }
    }

    #[test]
    fn create_reads_the_temperature_then_the_status_libusdr_logs() {
        // Golden session.trace lines 1441-1448.
        let (done, outcome) = drive(&mut create(TARGET), with_lock(2));
        assert_eq!(outcome, Ok(()));
        assert_eq!(
            done,
            [
                Exchange::Spi(0x0056_0000),
                Exchange::Spi(0x0018_0001),
                Exchange::Spi(0x0017_e040),
                Exchange::Sleep(1000),
                Exchange::Spi(0x0098_0000),
                Exchange::Spi(0x0098_0000),
                Exchange::Spi(0x00cb_0000),
                Exchange::Spi(0x00c1_0000),
            ]
        );
    }

    #[test]
    fn a_failed_status_read_does_not_fail_create() {
        let mut create = create(TARGET);
        let (done, outcome) = drive(&mut create, |action| match action {
            SequenceAction::Fpga(FpgaAccess::Spi {
                word: 0x00cb_0000, ..
            }) => Err(()),
            _ => Ok(0),
        });
        assert_eq!(
            outcome,
            Ok(()),
            "libusdr ignores lmx1204_read_status's result"
        );
        assert_eq!(done.len(), 7, "the VCO read after the failure is skipped");
    }

    #[test]
    fn configure_resets_then_writes_the_plan() {
        let (done, outcome) = drive(&mut configure(TARGET, &plan()), with_lock(0));
        assert_eq!(outcome, Ok(()));
        assert_eq!(
            done[..3],
            [
                Exchange::Spi(0x0000_0001),
                Exchange::Spi(0x0000_0000),
                Exchange::Sleep(10_000)
            ]
        );
        assert_eq!(done.len(), 3 + 28);
        assert_eq!(done.last(), Some(&Exchange::Spi(0x0000_0000)));
    }

    #[test]
    fn a_locked_multiplier_switches_the_state_machine_clock_off() {
        let mut wait = WaitLock::new(TARGET, &plan());
        let (done, outcome) = drive(&mut wait, with_lock(2));
        assert_eq!(outcome, Ok(()));
        assert_eq!(
            done,
            [Exchange::Spi(0x00cb_0000), Exchange::Spi(0x0002_0083)],
            "golden lines 1483-1484"
        );
        assert!(wait.locked());
    }

    #[test]
    fn an_unlocked_multiplier_times_out_after_a_second_and_keeps_its_clock() {
        let mut wait = WaitLock::new(TARGET, &plan());
        let (done, outcome) = drive(&mut wait, with_lock(3));
        assert_eq!(outcome, Ok(()));
        assert!(!wait.locked());
        assert_eq!(done.len(), 2 * 1000, "1000 reads, each followed by 1 ms");
    }
}
