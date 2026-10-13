//! A fixed list of actions run in order: the shape of the board's straight-line register
//! work (an LMS6002D field update, a run of GPO writes, a settling wait), the part every
//! composite machine is built from.

use core::time::Duration;

use super::sequence::{FpgaAccess, RegisterAccess, Sequence, SequenceAction, SequenceFailure};

/// The most actions one script holds: the LMS6002D's power-up configuration (a read and
/// thirteen writes) and the board's idle configuration after it, with room.
pub(crate) const SCRIPT_CAPACITY: usize = 24;

/// One action and the chip a failure of it names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Step {
    /// What to do.
    action: SequenceAction,
    /// The chip behind it, for failure reports.
    chip: &'static str,
}

/// Actions issued in order. By default the first failure abandons the rest, as a chain of
/// `?` does; [`Script::every`] attempts each whatever the others did. What reads return is
/// kept only for the last read. Fixed capacity, no allocation.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Script {
    /// The actions; the first `len` are used.
    steps: [Step; SCRIPT_CAPACITY],
    /// How many actions the script holds.
    len: usize,
    /// Actions completed, failed ones included.
    done: usize,
    /// Actions that failed.
    failures: usize,
    /// Attempt every action, whatever the others did.
    every: bool,
    /// The word the last read reported.
    read: Option<u32>,
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
            failures: 0,
            every: false,
            read: None,
        }
    }

    /// The same actions, each attempted whatever the others did, every failure reported:
    /// libusdr's power-downs and the driver's "attempt every step" stops.
    pub(crate) const fn every(mut self) -> Self {
        self.every = true;
        self
    }

    /// Appends `action` on `chip`.
    pub(crate) fn then(mut self, chip: &'static str, action: SequenceAction) -> Self {
        *self
            .steps
            .get_mut(self.len)
            .expect("invariant: no script composes past SCRIPT_CAPACITY") = Step { action, chip };
        self.len += 1;
        self
    }

    /// Appends an FPGA access on `chip`.
    pub(crate) fn fpga(self, chip: &'static str, access: FpgaAccess) -> Self {
        self.then(chip, SequenceAction::Fpga(access))
    }

    /// Appends an I²C register access on `chip`.
    pub(crate) fn i2c(self, chip: &'static str, access: RegisterAccess) -> Self {
        self.then(chip, SequenceAction::Access(access))
    }

    /// Appends a wait.
    pub(crate) fn sleep(self, duration: Duration) -> Self {
        self.then("", SequenceAction::Sleep(duration))
    }

    /// Appends `other`'s actions.
    pub(crate) fn extend(self, other: &Self) -> Self {
        other.steps[..other.len]
            .iter()
            .fold(self, |script, step| script.then(step.chip, step.action))
    }

    /// Whether every action has run without a failure.
    pub(crate) const fn succeeded(&self) -> bool {
        self.done == self.len && self.failures == 0
    }

    /// The word the last read reported, once one has.
    pub(crate) const fn read(&self) -> Option<u32> {
        self.read
    }

    /// The step in progress, if any.
    fn step(&self) -> Option<Step> {
        if self.failures > 0 && !self.every {
            return None;
        }
        self.steps[..self.len].get(self.done).copied()
    }
}

impl Default for Script {
    fn default() -> Self {
        Self::new()
    }
}

/// The failure `result` is for `action` on `chip`, or the word it reported; a wait
/// cannot fail.
pub(crate) fn checked<E>(
    chip: &'static str,
    action: SequenceAction,
    result: Result<u32, E>,
) -> Result<u32, SequenceFailure<E>> {
    result.or_else(|source| match action {
        SequenceAction::Sleep(_) => Ok(0),
        SequenceAction::Fpga(access) => Err(SequenceFailure::FpgaTransport {
            chip,
            access,
            source,
        }),
        SequenceAction::Access(access) => Err(SequenceFailure::Transport {
            chip,
            access,
            source,
        }),
    })
}

impl Sequence for Script {
    fn next(&self) -> Option<SequenceAction> {
        Some(self.step()?.action)
    }

    fn attempts_after_failure(&self) -> bool {
        self.every
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(step) = self.step() else {
            return Ok(());
        };
        self.done += 1;
        let word = checked(step.chip, step.action, result).inspect_err(|_| self.failures += 1)?;
        if matches!(
            step.action,
            SequenceAction::Fpga(FpgaAccess::Read { .. } | FpgaAccess::Spi { write: false, .. })
                | SequenceAction::Access(RegisterAccess::Read { .. })
        ) {
            self.read = Some(word);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIRST: FpgaAccess = FpgaAccess::Read { reg: 1 };
    const SECOND: FpgaAccess = FpgaAccess::Write { reg: 2, value: 3 };

    #[test]
    fn the_first_failure_abandons_the_rest() {
        let mut script = Script::new().fpga("FPGA", FIRST).fpga("FPGA", SECOND);
        assert!(script.complete(Err(())).is_err());
        assert_eq!(
            script.next(),
            None,
            "the write after the failed read is skipped"
        );
        assert!(!script.succeeded());
    }

    #[test]
    fn every_action_of_an_every_script_is_attempted() {
        let mut script = Script::new()
            .fpga("FPGA", FIRST)
            .fpga("FPGA", SECOND)
            .every();
        assert!(script.attempts_after_failure());
        assert!(script.complete(Err(())).is_err());
        assert_eq!(script.next(), Some(SequenceAction::Fpga(SECOND)));
        assert_eq!(script.complete::<()>(Ok(0)), Ok(()));
        assert_eq!(script.next(), None);
        assert!(!script.succeeded(), "one action failed");
    }

    #[test]
    fn the_last_read_is_kept_and_waits_cannot_fail() {
        let mut script = Script::new()
            .fpga("FPGA", FIRST)
            .sleep(Duration::from_micros(1));
        script.complete::<()>(Ok(7)).unwrap();
        assert_eq!(script.complete(Err(())), Ok(()), "a wait cannot fail");
        assert_eq!(script.read(), Some(7));
        assert!(script.succeeded());
    }
}
