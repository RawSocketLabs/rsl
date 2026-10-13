//! One access as a sequence, for the steps libusdr runs alone: the liveness read, and the
//! close's writes, each of which it attempts whatever the others did.

use rsl_lowlevel::{Sequence, SequenceAction, SequenceFailure};

use super::outcome::outcome;

/// A single action on `chip`; keeps the value it reported.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Single {
    /// The action, until it has been completed.
    action: Option<SequenceAction>,

    /// The device an I²C access or SPI exchange reaches, for its failure.
    chip: &'static str,

    /// The value reported; zero until then.
    value: u32,
}

impl Single {
    /// The sequence of `action` alone.
    pub(crate) const fn new(action: SequenceAction, chip: &'static str) -> Self {
        Self {
            action: Some(action),
            chip,
            value: 0,
        }
    }

    /// The value the action reported: the word or byte read, once the sequence finished.
    pub(crate) const fn value(&self) -> u32 {
        self.value
    }
}

impl Sequence for Single {
    fn next(&self) -> Option<SequenceAction> {
        self.action
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(action) = self.action.take() else {
            return Ok(());
        };
        self.value = outcome(action, result, self.chip)?;
        Ok(())
    }
}
