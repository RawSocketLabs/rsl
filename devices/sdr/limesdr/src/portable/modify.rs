//! A gateware register's read-modify-write, the pattern `limesdr_ctrl.c` uses for every
//! control bit it changes: read the register, clear some bits, set others, write it back.

use hal::{FpgaAccess, Sequence, SequenceAction, SequenceFailure};

use super::fpga;

/// Where the read-modify-write is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    /// Reading the register.
    Read,
    /// Writing the new value.
    Write(u16),
    /// Finished.
    Done,
    /// Stopped by a failure.
    Failed,
}

/// One gateware register's read-modify-write: `(value & !clear) | set`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Modify {
    /// The register.
    reg: u16,
    /// The bits cleared.
    clear: u16,
    /// The bits set.
    set: u16,
    /// Where it is.
    step: Step,
}

impl Modify {
    /// Starts the read-modify-write of `reg`.
    pub(crate) const fn new(reg: u16, clear: u16, set: u16) -> Self {
        Self {
            reg,
            clear,
            set,
            step: Step::Read,
        }
    }

    /// The access the current step performs.
    const fn access(&self) -> Option<FpgaAccess> {
        let reg = self.reg as u32;
        match self.step {
            Step::Read => Some(FpgaAccess::Read { reg }),
            Step::Write(value) => Some(FpgaAccess::Write {
                reg,
                value: value as u32,
            }),
            Step::Done | Step::Failed => None,
        }
    }
}

impl Sequence for Modify {
    fn next(&self) -> Option<SequenceAction> {
        self.access().map(SequenceAction::Fpga)
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(access) = self.access() else {
            return Ok(());
        };
        let value = result.map_err(|source| {
            self.step = Step::Failed;
            SequenceFailure::FpgaTransport {
                chip: fpga::NAME,
                access,
                source,
            }
        })?;
        self.step = match self.step {
            Step::Read => {
                // A 16-bit register, zero-extended.
                let [low, high, ..] = value.to_le_bytes();
                let read = u16::from_le_bytes([low, high]);
                Step::Write(read & !self.clear | self.set)
            }
            Step::Write(_) | Step::Done | Step::Failed => Step::Done,
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use hal::Access;

    use super::*;

    #[test]
    fn only_the_cleared_and_set_bits_change() {
        let mut modify = Modify::new(0x0017, 0b11 << 8, 0b10 << 8);
        assert_eq!(
            modify.next(),
            Some(SequenceAction::Fpga(FpgaAccess::Read { reg: 0x0017 }))
        );
        modify.complete::<()>(Ok(0xf3ff)).expect("read");
        assert_eq!(
            modify.next(),
            Some(SequenceAction::Fpga(FpgaAccess::Write {
                reg: 0x0017,
                value: 0xf2ff
            }))
        );
        modify.complete::<()>(Ok(0)).expect("written");
        assert_eq!(modify.next(), None);
    }

    #[test]
    fn a_failed_read_writes_nothing_and_names_the_register() {
        let mut modify = Modify::new(0x0005, 0, 1);
        let failure = modify.complete(Err(())).expect_err("the read failed");
        assert_eq!(failure.access(), Some(("FPGA", Access::Read, 5)));
        assert_eq!(modify.next(), None);
    }
}
