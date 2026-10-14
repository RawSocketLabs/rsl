//! One action as a sequence: a gateware register access, an I²C register write or a wait,
//! for the board's parts of longer machines.

use core::time::Duration;

use hal::{FpgaAccess, RegisterAccess, Sequence, SequenceAction, SequenceFailure};

use super::fpga;
use super::outcome::outcome;

/// One action and what it reported.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Step {
    /// The action.
    action: SequenceAction,

    /// The chip an I²C access names in a failure.
    chip: &'static str,

    /// The value reported, once performed.
    value: Option<u32>,

    /// It failed.
    failed: bool,
}

impl Step {
    /// A gateware register access.
    pub(crate) const fn fpga(access: FpgaAccess) -> Self {
        Self::new(SequenceAction::Fpga(access), fpga::NAME)
    }

    /// An I²C register access on `chip`.
    pub(crate) const fn i2c(access: RegisterAccess, chip: &'static str) -> Self {
        Self::new(SequenceAction::Access(access), chip)
    }

    /// A wait of `micros` microseconds.
    pub(crate) const fn wait(micros: u64) -> Self {
        Self::new(
            SequenceAction::Sleep(Duration::from_micros(micros)),
            fpga::NAME,
        )
    }

    /// `action`, not yet performed.
    const fn new(action: SequenceAction, chip: &'static str) -> Self {
        Self {
            action,
            chip,
            value: None,
            failed: false,
        }
    }

    /// The value the action reported, once performed.
    pub(crate) const fn value(&self) -> Option<u32> {
        self.value
    }
}

impl Sequence for Step {
    fn next(&self) -> Option<SequenceAction> {
        (self.value.is_none() && !self.failed).then_some(self.action)
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(action) = self.next() else {
            return Ok(());
        };
        let value = outcome(action, result, self.chip).inspect_err(|_| self.failed = true)?;
        self.value = Some(value);
        Ok(())
    }
}
