//! Sequences built from sequences: one after another, and one whose failure libusdr ignores.

use rsl_lowlevel::{Sequence, SequenceAction, SequenceFailure};

/// `first`, then `second`; a failure of `first` ends both (the C's `if (res) return res`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Then<A, B> {
    /// Runs first.
    pub(crate) first: A,
    /// Runs once `first` has finished without failing.
    pub(crate) second: B,
    /// `first` failed.
    failed: bool,
}

impl<A: Sequence, B: Sequence> Then<A, B> {
    /// `first`, then `second`.
    pub(crate) const fn new(first: A, second: B) -> Self {
        Self {
            first,
            second,
            failed: false,
        }
    }
}

impl<A: Sequence, B: Sequence> Sequence for Then<A, B> {
    fn next(&self) -> Option<SequenceAction> {
        if self.failed {
            return None;
        }
        self.first.next().or_else(|| self.second.next())
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        if self.failed {
            return Ok(());
        }
        if self.first.next().is_some() {
            return self
                .first
                .complete(result)
                .inspect_err(|_| self.failed = true);
        }
        self.second.complete(result)
    }
}

/// `inner`, whose failure ends it quietly: libusdr calls these "just for logging" and
/// ignores their result (`lmx1204_read_status`, `lmx2820_read_status`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Lenient<S>(pub(crate) S);

impl<S: Sequence> Sequence for Lenient<S> {
    fn next(&self) -> Option<SequenceAction> {
        self.0.next()
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        // The inner sequence has stopped at its failure; the caller carries on.
        let _ignored = self.0.complete(result);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rsl_lowlevel::FpgaAccess;

    use super::*;
    use crate::portable::script::Script;

    const FIRST: SequenceAction = SequenceAction::Fpga(FpgaAccess::Read { reg: 1 });
    const SECOND: SequenceAction = SequenceAction::Fpga(FpgaAccess::Read { reg: 2 });

    fn pair() -> Then<Script, Script> {
        Then::new(
            Script::new().then("FPGA", FIRST),
            Script::new().then("FPGA", SECOND),
        )
    }

    #[test]
    fn the_second_runs_after_the_first_succeeds() {
        let mut both = pair();
        both.complete::<()>(Ok(0)).unwrap();
        assert_eq!(both.next(), Some(SECOND));
    }

    #[test]
    fn a_failed_first_ends_both() {
        let mut both = pair();
        assert!(both.complete(Err(())).is_err());
        assert_eq!(both.next(), None);
    }

    #[test]
    fn a_lenient_sequence_stops_at_its_failure_without_reporting_it() {
        let mut quiet = Lenient(Script::new().then("FPGA", FIRST).then("FPGA", SECOND));
        assert_eq!(quiet.complete::<()>(Err(())), Ok(()));
        assert_eq!(quiet.next(), None, "the read after the failure is skipped");
    }
}
