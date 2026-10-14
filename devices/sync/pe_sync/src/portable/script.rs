//! A fixed list of actions run in order: the shape of libusdr's register lists
//! (`common_spi_post`) and of its `res = res ? res : ...` chains, where the first failure
//! ends the list.

use core::time::Duration;

use hal::{Sequence, SequenceAction, SequenceFailure};

use super::outcome::outcome;

/// The most actions one script holds: `lmx1204_solver`'s 28 register writes, with room.
pub(crate) const SCRIPT_CAPACITY: usize = 32;

/// One action and the chip a failure of it names.
#[derive(Clone, Copy, Debug)]
struct Step {
    /// What to do.
    action: SequenceAction,
    /// The device behind it, for failure reports.
    chip: &'static str,
}

/// Actions issued in order; the first failure abandons the rest. What reads return is not
/// kept. Fixed capacity, no allocation.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Script {
    /// The actions; the first `len` are used.
    steps: [Step; SCRIPT_CAPACITY],
    /// How many actions the script holds.
    len: usize,
    /// Actions completed.
    done: usize,
    /// An action failed; nothing more is issued.
    failed: bool,
}

impl Script {
    /// An empty script.
    pub(crate) const fn new() -> Self {
        let unused = Step {
            action: SequenceAction::Sleep(Duration::ZERO),
            chip: "",
        };
        Self {
            steps: [unused; SCRIPT_CAPACITY],
            len: 0,
            done: 0,
            failed: false,
        }
    }

    /// Appends `action`, whose failure names `chip`.
    ///
    /// # Panics
    ///
    /// Past [`SCRIPT_CAPACITY`]: every script this crate builds has a fixed size within it.
    pub(crate) const fn then(mut self, chip: &'static str, action: SequenceAction) -> Self {
        assert!(
            self.len < SCRIPT_CAPACITY,
            "invariant: scripts fit their capacity"
        );
        self.steps[self.len] = Step { action, chip };
        self.len += 1;
        self
    }

    /// Appends every action of `other`, which has not started.
    ///
    /// # Panics
    ///
    /// Past [`SCRIPT_CAPACITY`], as [`Script::then`].
    pub(crate) const fn append(mut self, other: Self) -> Self {
        let mut index = 0;
        while index < other.len {
            let step = other.steps[index];
            self = self.then(step.chip, step.action);
            index += 1;
        }
        self
    }

    /// The step to perform next.
    fn step(&self) -> Option<Step> {
        if self.failed || self.done >= self.len {
            return None;
        }
        Some(self.steps[self.done])
    }
}

impl Sequence for Script {
    fn next(&self) -> Option<SequenceAction> {
        self.step().map(|step| step.action)
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(step) = self.step() else {
            return Ok(());
        };
        outcome(step.action, result, step.chip).inspect_err(|_| self.failed = true)?;
        self.done += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use hal::{Access, FpgaAccess};

    use super::*;

    const WRITE: SequenceAction = SequenceAction::Fpga(FpgaAccess::Write { reg: 0, value: 1 });
    const READ: SequenceAction = SequenceAction::Fpga(FpgaAccess::Read { reg: 20 });

    #[test]
    fn a_failure_names_its_action_and_abandons_the_rest() {
        let mut script = Script::new().then("FPGA", WRITE).then("FPGA", READ);
        assert_eq!(script.complete::<()>(Ok(0)), Ok(()));
        let failure = script.complete(Err(())).unwrap_err();
        assert_eq!(failure.access(), Some(("FPGA", Access::Read, 20)));
        assert_eq!(script.next(), None, "nothing follows the failure");
    }

    #[test]
    fn a_failed_wait_still_counts_as_waited() {
        let wait = SequenceAction::Sleep(Duration::from_millis(1));
        let mut script = Script::new().then("FPGA", wait).then("FPGA", WRITE);
        assert_eq!(script.complete::<()>(Err(())), Ok(()));
        assert_eq!(script.next(), Some(WRITE));
    }

    #[test]
    fn an_empty_script_issues_nothing() {
        assert_eq!(Script::new().next(), None);
    }
}
