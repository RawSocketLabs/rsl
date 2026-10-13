//! How the board's composite operations run their parts: one at a time, each part a
//! sequence (a script, a tuning, another composite), with libusdr's three ways of meeting a
//! failure: stop (`?`), carry on and report it last (`ret = ret ? ret : ...`, a settling
//! wait taken even after a failed write), or ignore it (`(void)`, a discarded result).

use super::super::lms6002d::TuneFault;
use super::super::sequence::{Sequence, SequenceAction, SequenceFailure};
use super::state::Radio;

/// What a composite does when a part reports a failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Policy {
    /// Report it and stop once the part has ended.
    Stop,
    /// Report it, then go on to the next part.
    Continue,
    /// Discard it and go on, as libusdr discards the result.
    Ignore,
}

/// A composite operation's steps: which part comes next, given how the last one ended,
/// and what each part's failure means. The steps own the state the parts change.
pub(crate) trait Steps {
    /// The parts it runs.
    type Part: Sequence;

    /// The first part, or the next once the last ended: `failed` if it reported a failure
    /// it did not stop on. `None` ends the operation.
    fn next_part(&mut self, failed: bool) -> Option<Self::Part>;

    /// Takes back what `part` changed, once it has ended, whatever its outcome.
    fn absorb(&mut self, part: &Self::Part) {
        let _ = part;
    }

    /// What a failure of the current part means.
    fn policy<E>(&self, failure: &SequenceFailure<E>) -> Policy {
        let _ = failure;
        Policy::Stop
    }

    /// The fault that ended the operation, if one did; once set, [`Self::next_part`]
    /// returns `None`.
    fn fault(&self) -> Option<TuneFault> {
        None
    }
}

/// Steps that own the board's [`Radio`] state while they run.
pub(crate) trait OwnsRadio {
    /// The state as the steps have left it so far.
    fn radio(&self) -> &Radio;
}

/// Implements [`OwnsRadio`] for steps keeping the state in their `radio` field.
macro_rules! owns_radio {
    ($($ty:ty),+) => {
        $(impl $crate::portable::radio::run::OwnsRadio for $ty {
            fn radio(&self) -> &$crate::portable::radio::state::Radio {
                &self.radio
            }
        })+
    };
}
pub(crate) use owns_radio;

/// A composite operation in progress: its steps and the part running.
#[derive(Clone, Copy, Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent facts about the part running and the run's failures"
)]
pub(crate) struct Run<S: Steps> {
    /// The steps.
    steps: S,
    /// The part running, `None` once finished.
    part: Option<S::Part>,
    /// The running part reported a failure.
    failed: bool,
    /// The running part's failure stops the operation once it ends.
    stopping: bool,
    /// A failure has been reported.
    reported: bool,
    /// The steps' fault came before any reported failure.
    fault_first: bool,
}

impl<S: Steps> Run<S> {
    /// Starts `steps` at their first part.
    pub(crate) fn new(mut steps: S) -> Self {
        let part = steps.next_part(false);
        let mut run = Self {
            steps,
            part,
            failed: false,
            stopping: false,
            reported: false,
            fault_first: false,
        };
        run.note_fault();
        run.settle();
        run
    }

    /// The steps, and the state they own, as the operation has left them so far.
    pub(crate) const fn steps(&self) -> &S {
        &self.steps
    }

    /// The state the steps own, as the operation has left it so far.
    pub(crate) fn radio(&self) -> Radio
    where
        S: OwnsRadio,
    {
        *self.steps.radio()
    }

    /// Whether a failure was reported: the run did not complete.
    pub(crate) const fn reported(&self) -> bool {
        self.reported
    }

    /// The fault that ended the operation, unless a reported failure came first; check it
    /// once the run has ended without a failure, or with one, as the first of the two
    /// counts.
    pub(crate) fn fault(&self) -> Option<TuneFault> {
        self.fault_first.then(|| self.steps.fault()).flatten()
    }

    /// Notes whether the steps' fault came before any reported failure.
    fn note_fault(&mut self) {
        if !self.reported && self.steps.fault().is_some() {
            self.fault_first = true;
        }
    }

    /// Moves past ended parts until one has an action or the operation has finished.
    fn settle(&mut self) {
        while let Some(part) = &self.part {
            if part.next().is_some() {
                return;
            }
            self.steps.absorb(part);
            self.note_fault();
            self.part = if self.stopping || self.fault_first {
                None
            } else {
                self.steps.next_part(self.failed)
            };
            self.failed = false;
            self.note_fault();
        }
    }
}

impl<S: Steps> Sequence for Run<S> {
    fn next(&self) -> Option<SequenceAction> {
        self.part.as_ref()?.next()
    }

    fn attempts_after_failure(&self) -> bool {
        true
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(part) = &mut self.part else {
            return Ok(());
        };
        let outcome = match part.complete(result) {
            Ok(()) => Ok(()),
            Err(failure) => match self.steps.policy(&failure) {
                Policy::Ignore => Ok(()),
                policy => {
                    self.failed = true;
                    self.stopping |= policy == Policy::Stop;
                    if self.reported || self.fault_first {
                        Ok(())
                    } else {
                        self.reported = true;
                        Err(failure)
                    }
                }
            },
        };
        self.settle();
        outcome
    }
}

/// An enum of the parts a composite runs, as one [`Sequence`] delegating to its variant.
macro_rules! parts {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident { $($(#[$vmeta:meta])* $variant:ident($ty:ty)),+ $(,)? }
    ) => {
        $(#[$meta])*
        #[allow(
            clippy::large_enum_variant,
            reason = "fixed-size parts, no allocation; only some part sets are uneven"
        )]
        $vis enum $name {
            $($(#[$vmeta])* $variant($ty)),+
        }

        impl $crate::portable::sequence::Sequence for $name {
            fn next(&self) -> Option<$crate::portable::sequence::SequenceAction> {
                match self {
                    $(Self::$variant(part) => $crate::portable::sequence::Sequence::next(part)),+
                }
            }

            fn attempts_after_failure(&self) -> bool {
                match self {
                    $(Self::$variant(part) => {
                        $crate::portable::sequence::Sequence::attempts_after_failure(part)
                    }),+
                }
            }

            fn complete<E>(
                &mut self,
                result: Result<u32, E>,
            ) -> Result<(), $crate::portable::sequence::SequenceFailure<E>> {
                match self {
                    $(Self::$variant(part) => {
                        $crate::portable::sequence::Sequence::complete(part, result)
                    }),+
                }
            }
        }
    };
}
pub(crate) use parts;

#[cfg(test)]
mod tests {
    use super::super::super::script::Script;
    use super::super::super::sequence::FpgaAccess;
    use super::*;

    const READ: FpgaAccess = FpgaAccess::Read { reg: 1 };

    /// Two one-read scripts, the first's failure met with `policy`.
    struct Two {
        policy: Policy,
        parts: u8,
        failed_before_second: Option<bool>,
    }

    impl Steps for Two {
        type Part = Script;

        fn next_part(&mut self, failed: bool) -> Option<Script> {
            self.parts += 1;
            match self.parts {
                1 => Some(Script::new().fpga("FPGA", READ)),
                2 => {
                    self.failed_before_second = Some(failed);
                    Some(Script::new().fpga("FPGA", READ))
                }
                _ => None,
            }
        }

        fn policy<E>(&self, _: &SequenceFailure<E>) -> Policy {
            self.policy
        }
    }

    /// Runs `Two` with `policy`, failing the first read; the reads made and the first
    /// failure reported.
    fn run(policy: Policy) -> (u8, bool, Option<bool>) {
        let mut run = Run::new(Two {
            policy,
            parts: 0,
            failed_before_second: None,
        });
        let mut reads = 0;
        let mut reported = false;
        let mut result = Err(());
        while run.next().is_some() {
            reads += 1;
            reported |= run.complete(result).is_err();
            result = Ok(0);
        }
        (reads, reported, run.steps().failed_before_second)
    }

    #[test]
    fn a_stopping_failure_ends_the_operation_and_is_reported() {
        assert_eq!(run(Policy::Stop), (1, true, None));
    }

    #[test]
    fn a_continuing_failure_is_reported_and_the_next_part_learns_of_it() {
        assert_eq!(run(Policy::Continue), (2, true, Some(true)));
    }

    #[test]
    fn an_ignored_failure_is_neither_reported_nor_passed_on() {
        assert_eq!(run(Policy::Ignore), (2, false, Some(false)));
    }
}
