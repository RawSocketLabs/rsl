//! One part of a control-code operation: chip calls, a tuning or a board action.

use hal::{Sequence, SequenceAction, SequenceFailure};

use super::calls::Calls;
use crate::chip::{CgenTune, Lms7002m, SxxTune};

/// The part a control machine is running. `B` is the board's own sequence (the antenna
/// switch); machines without one use [`NoBoard`].
#[derive(Clone, Copy, Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "fixed-size parts, no allocation: the chip-call queue is the largest"
)]
pub(crate) enum Part<B> {
    /// Chip calls.
    Calls(Calls),

    /// A clock-generator tuning.
    Cgen(CgenTune),

    /// An LO synthesizer tuning.
    Sxx(SxxTune),

    /// A board action.
    Board(B),
}

impl<B: Sequence> Part<B> {
    /// Chip calls, built up to their first write.
    pub(crate) fn calls(mut calls: Calls, chip: &mut Lms7002m) -> Self {
        calls.start(chip);
        Self::Calls(calls)
    }

    /// The next action.
    pub(crate) fn next(&self) -> Option<SequenceAction> {
        match self {
            Self::Calls(calls) => calls.next(),
            Self::Cgen(tune) => tune.next(),
            Self::Sxx(tune) => tune.next(),
            Self::Board(board) => board.next(),
        }
    }

    /// Applies an action's outcome.
    pub(crate) fn complete<E>(
        &mut self,
        chip: &mut Lms7002m,
        result: Result<u32, E>,
    ) -> Result<(), SequenceFailure<E>> {
        match self {
            Self::Calls(calls) => calls.complete(chip, result),
            Self::Cgen(tune) => tune.complete(result),
            Self::Sxx(tune) => tune.complete(result),
            Self::Board(board) => board.complete(result),
        }
    }
}

/// The board sequence of a machine that has none.
#[derive(Clone, Copy, Debug)]
pub enum NoBoard {}

impl Sequence for NoBoard {
    fn next(&self) -> Option<SequenceAction> {
        match *self {}
    }

    fn complete<E>(&mut self, _result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        match *self {}
    }
}
