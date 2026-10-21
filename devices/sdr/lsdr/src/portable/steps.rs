//! A fixed list of actions as a sequence that ends at its first failure. The NCO's writes
//! use it; there libusdr ignores a failed second-mixer write and carries on, so ending
//! there is a divergence (`PORTING.md`).

use hal::{Sequence, SequenceAction, SequenceFailure};

use super::outcome::outcome;

/// `N` actions, each with the chip it reaches, run in order; the first failure ends them.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Steps<const N: usize> {
    /// The actions and their chips.
    actions: [(SequenceAction, &'static str); N],

    /// Actions completed.
    done: usize,

    /// An action failed; nothing more is issued.
    failed: bool,
}

impl<const N: usize> Steps<N> {
    /// The sequence of `actions`; nothing has been accessed yet.
    pub(crate) const fn new(actions: [(SequenceAction, &'static str); N]) -> Self {
        Self {
            actions,
            done: 0,
            failed: false,
        }
    }

    /// The pending action and its chip.
    fn pending(&self) -> Option<(SequenceAction, &'static str)> {
        self.actions
            .get(self.done)
            .copied()
            .filter(|_| !self.failed)
    }
}

impl<const N: usize> Sequence for Steps<N> {
    fn next(&self) -> Option<SequenceAction> {
        self.pending().map(|(action, _)| action)
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some((action, chip)) = self.pending() else {
            return Ok(());
        };
        outcome(action, result, chip).inspect_err(|_| self.failed = true)?;
        self.done += 1;
        Ok(())
    }
}
