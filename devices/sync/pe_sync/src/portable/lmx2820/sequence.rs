//! The LMX2820's bus operations as `hw/lmx2820/lmx2820.c` runs them: `lmx2820_create`,
//! `lmx2820_tune` (without instant calibration) and `lmx2820_read_status`.

use core::time::Duration;

use bnb::{u2, u11};
use rsl_lowlevel::{Sequence, SequenceAction, SequenceFailure, SpiAddr};

use super::plan::Plan;
use super::reg::{Control, LockDetect, LockStatus, Reg, TemperatureSensor};
use crate::portable::chain::Then;
use crate::portable::outcome::outcome;
use crate::portable::poll::{LockPoll, Wait};
use crate::portable::script::Script;
use crate::portable::ti_spi::{self, Register};

/// The chip's name in errors.
pub(crate) const NAME: &str = "LMX2820";

/// The pause after a reset and after enabling the temperature sensor, and after a tune's
/// registers ("to allow the internal LDOs to power up").
const SETTLE: Duration = Duration::from_millis(10);

/// The lock wait's poll interval (`usleep(100)` in `lmx2820_wait_pll_lock`).
const LOCK_INTERVAL: Duration = Duration::from_micros(100);

/// The lock wait `lmx2820_tune_internal` allows (`lmx2820_wait_pll_lock(st, 1000000)`).
const LOCK_TIMEOUT: Duration = Duration::from_secs(1);

/// `lmx2820_read_status`: the temperature, lock and VCO readbacks, which libusdr only logs.
pub(crate) const fn status(target: SpiAddr) -> Script {
    Script::new()
        .then(NAME, ti_spi::read(target, Reg::Temperature as u8))
        .then(NAME, ti_spi::read(target, Reg::LockStatus as u8))
        .then(NAME, ti_spi::read(target, Reg::VcoCurrent as u8))
}

/// `lmx2820_create`: a reset (R0 read, then written with `RESET` set and cleared), the
/// temperature sensor on, and a status read whose failure fails the create.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Create {
    /// The chip.
    target: SpiAddr,
    /// Everything after the R0 read, once it is done.
    rest: Option<Script>,
    /// The R0 read failed.
    failed: bool,
}

impl Create {
    /// Starts the sequence.
    pub(crate) const fn new(target: SpiAddr) -> Self {
        Self {
            target,
            rest: None,
            failed: false,
        }
    }

    /// The R0 read the reset starts from (`lmx2820_reset`).
    const fn read_control(&self) -> SequenceAction {
        ti_spi::read(self.target, Reg::Control as u8)
    }

    /// The reset from `control`, the sensor enable and the status read.
    fn rest(&self, control: Control) -> Script {
        let write = |value: Control| ti_spi::write(self.target, value.entry());
        let wait = SequenceAction::Sleep(SETTLE);
        let sensor = TemperatureSensor::new()
            .with_reserved(u11::new(0x109))
            .with_enable(u2::new(3));
        Script::new()
            .then(NAME, write(control.with_reset(true)))
            .then(NAME, write(control.with_reset(false)))
            // "reset takes <1us", libusdr waits 10 ms.
            .then(NAME, wait)
            .then(NAME, ti_spi::write(self.target, sensor.entry()))
            .then(NAME, wait)
            .append(status(self.target))
    }
}

impl Sequence for Create {
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
        self.rest = Some(self.rest(Control::from_raw(ti_spi::value(word))));
        Ok(())
    }
}

/// `lmx2820_tune` without instant calibration: the plan's registers, 10 ms, then the lock
/// wait. A timed-out wait ends the sequence without failure; [`Tune::locked`] says so.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tune(Then<Script, LockPoll>);

impl Tune {
    /// Starts tuning the chip at `target` to `plan`.
    pub(crate) fn new(target: SpiAddr, plan: &Plan) -> Self {
        let mut writes = Script::new();
        for entry in plan.registers() {
            writes = writes.then(NAME, ti_spi::write(target, entry));
        }
        let lock = LockPoll::new(
            NAME,
            ti_spi::read(target, Reg::LockStatus as u8),
            |value| LockStatus::from_raw(value).lock() == LockDetect::Locked,
            LOCK_INTERVAL,
            LOCK_TIMEOUT,
        );
        Self(Then::new(
            writes.then(NAME, SequenceAction::Sleep(SETTLE)),
            lock,
        ))
    }

    /// Whether the PLL reported lock; `false` until the wait has ended.
    pub(crate) fn locked(&self) -> bool {
        self.0.second.outcome() == Wait::Locked
    }
}

impl Sequence for Tune {
    fn next(&self) -> Option<SequenceAction> {
        self.0.next()
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        self.0.complete(result)
    }
}

#[cfg(test)]
mod tests {
    use rsl_lowlevel::Access;

    use super::super::plan::Request;
    use super::super::reg::MashOrder;
    use super::*;
    use crate::portable::testing::{Exchange, drive};

    /// The board's LMX2820 #0 bus.
    const TARGET: SpiAddr = SpiAddr(2);

    #[test]
    fn create_resets_from_r0_and_enables_the_sensor_as_libusdr_does() {
        // Golden session.trace lines 1340-1348, R0 reading its reset value 0x4070.
        let (done, outcome) = drive(&mut Create::new(TARGET), |_| Ok(0x4070));
        assert_eq!(outcome, Ok(()));
        assert_eq!(
            done,
            [
                Exchange::Spi(0x0080_0000),
                Exchange::Spi(0x0000_4072),
                Exchange::Spi(0x0000_4070),
                Exchange::Sleep(10_000),
                Exchange::Spi(0x0013_2138),
                Exchange::Sleep(10_000),
                Exchange::Spi(0x00cc_0000),
                Exchange::Spi(0x00ca_0000),
                Exchange::Spi(0x00cb_0000),
            ]
        );
    }

    #[test]
    fn a_failed_r0_read_ends_create_before_any_write() {
        let mut create = Create::new(TARGET);
        let failure = create.complete(Err(())).unwrap_err();
        assert_eq!(failure.access(), Some((NAME, Access::Read, 0x00)));
        assert_eq!(create.next(), None);
    }

    /// `PE_SYNC`'s LMX2820 #0 plan.
    fn plan() -> Plan {
        Plan::solve(&Request {
            reference_hz: 150_000_000,
            mash: MashOrder::Second,
            multiplier: None,
            out_a_hz: 1_000_000_000,
            out_b_hz: 1_000_000_000,
        })
        .expect("libusdr's plan")
    }

    #[test]
    fn tune_writes_its_registers_waits_and_polls_lock_once_when_locked() {
        let mut tune = Tune::new(TARGET, &plan());
        let (done, outcome) = drive(&mut tune, |_| Ok(0x8000));
        assert_eq!(outcome, Ok(()));
        assert_eq!(done.len(), 25 + 2, "the registers, 10 ms, one lock read");
        assert_eq!(done[25], Exchange::Sleep(10_000));
        assert_eq!(done[26], Exchange::Spi(0x00ca_0000));
        assert!(tune.locked());
    }

    #[test]
    fn an_unlocked_pll_times_out_after_a_second_of_polls() {
        let mut tune = Tune::new(TARGET, &plan());
        let (done, outcome) = drive(&mut tune, |_| Ok(0x4000));
        assert_eq!(
            outcome,
            Ok(()),
            "a timeout is reported by `locked`, not as a failure"
        );
        assert!(!tune.locked());
        assert_eq!(
            done.len(),
            26 + 2 * 10_000,
            "10 000 reads, each followed by 100 µs"
        );
    }
}
