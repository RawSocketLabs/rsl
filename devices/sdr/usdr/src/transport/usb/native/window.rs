//! Portable sequences whose registers sit in the stream engine's configuration space, run
//! one physical access at a time: the native executor's counterpart of the blocking bus's
//! index window (`transport/window.rs`), so both reach the same registers in the same
//! order.

use super::super::bus::WINDOW;
use crate::portable::sequence::{FpgaAccess, Sequence, SequenceAction, SequenceFailure};
use crate::transport::window::Window;

/// A sequence whose accesses to registers at or past the window's first go through the
/// index/data window, as `UsbBus` performs them: the register's offset written to the
/// index register, then the access at the data register. Every other action passes
/// through unchanged. A failed index write is the access's failure, as the blocking bus
/// reports it.
#[derive(Debug)]
pub(super) struct Windowed<S> {
    /// The sequence, naming registers as the bus numbers them.
    inner: S,
    /// The transport's window.
    window: Window,
    /// The current access's index is written; its data access is next.
    indexed: bool,
}

impl<S: Sequence> Windowed<S> {
    /// `inner` through the uSDR's window.
    pub(super) const fn new(inner: S) -> Self {
        Self {
            inner,
            window: WINDOW,
            indexed: false,
        }
    }

    /// The offset of the inner sequence's next access, if it is in the window.
    fn offset(&self) -> Option<u32> {
        let Some(SequenceAction::Fpga(FpgaAccess::Read { reg } | FpgaAccess::Write { reg, .. })) =
            self.inner.next()
        else {
            return None;
        };
        reg.checked_sub(self.window.first)
    }
}

impl<S: Sequence> Sequence for Windowed<S> {
    fn next(&self) -> Option<SequenceAction> {
        let action = self.inner.next()?;
        let Some(offset) = self.offset() else {
            return Some(action);
        };
        let data = self.window.data();
        let access = match action {
            _ if !self.indexed => FpgaAccess::Write {
                reg: self.window.index,
                value: offset,
            },
            SequenceAction::Fpga(FpgaAccess::Write { value, .. }) => {
                FpgaAccess::Write { reg: data, value }
            }
            _ => FpgaAccess::Read { reg: data },
        };
        Some(SequenceAction::Fpga(access))
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        if self.offset().is_some() && !self.indexed && result.is_ok() {
            self.indexed = true;
            return Ok(());
        }
        self.indexed = false;
        self.inner.complete(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::stream::EngineSequence;

    /// The physical accesses `sequence` makes, every read answered with `word`.
    fn physical(mut sequence: impl Sequence, word: u32) -> Vec<FpgaAccess> {
        let mut accesses = Vec::new();
        while let Some(SequenceAction::Fpga(access)) = sequence.next() {
            accesses.push(access);
            sequence.complete::<()>(Ok(word)).unwrap();
        }
        accesses
    }

    #[test]
    fn configuration_registers_are_indexed_then_reached_at_the_data_register() {
        let halt = Windowed::new(EngineSequence::new().run_rx(false));
        assert_eq!(
            physical(halt, 0),
            [
                FpgaAccess::Write { reg: 5, value: 0 },
                // The front end, 0x10100: its offset, then the command.
                FpgaAccess::Write {
                    reg: 6,
                    value: 0x100,
                },
                FpgaAccess::Write {
                    reg: 7,
                    value: 0x3000_0004,
                },
            ]
        );
    }

    /// Reads configuration register 0x10040, then writes 9 to it.
    #[derive(Default)]
    struct ReadThenWrite {
        /// Accesses completed, and the word read.
        done: Vec<u32>,
    }

    impl Sequence for ReadThenWrite {
        fn next(&self) -> Option<SequenceAction> {
            let access = match self.done.len() {
                0 => FpgaAccess::Read { reg: 0x1_0040 },
                1 => FpgaAccess::Write {
                    reg: 0x1_0040,
                    value: 9,
                },
                _ => return None,
            };
            Some(SequenceAction::Fpga(access))
        }

        fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
            let Some(SequenceAction::Fpga(access)) = self.next() else {
                return Ok(());
            };
            let word = result.map_err(|source| SequenceFailure::FpgaTransport {
                chip: "FPGA",
                access,
                source,
            })?;
            self.done.push(word);
            Ok(())
        }
    }

    #[test]
    fn a_configuration_read_reports_the_data_registers_word() {
        let mut read = Windowed::new(ReadThenWrite::default());
        let mut accesses = Vec::new();
        while let Some(SequenceAction::Fpga(access)) = read.next() {
            accesses.push(access);
            let word = match access {
                FpgaAccess::Read { reg: 7 } => 0xabcd,
                _ => 0,
            };
            read.complete::<()>(Ok(word)).unwrap();
        }
        assert_eq!(
            accesses,
            [
                FpgaAccess::Write {
                    reg: 6,
                    value: 0x40
                },
                FpgaAccess::Read { reg: 7 },
                FpgaAccess::Write {
                    reg: 6,
                    value: 0x40
                },
                FpgaAccess::Write { reg: 7, value: 9 },
            ],
            "the window does not auto-increment: every access sets the index"
        );
        assert_eq!(
            read.inner.done,
            [0xabcd, 0],
            "the inner sequence saw the word"
        );
    }

    #[test]
    fn a_failed_index_write_fails_the_configuration_access_it_was_for() {
        let mut read = Windowed::new(ReadThenWrite::default());
        assert_eq!(
            read.complete(Err(())),
            Err(SequenceFailure::FpgaTransport {
                chip: "FPGA",
                access: FpgaAccess::Read { reg: 0x1_0040 },
                source: (),
            }),
            "named as the blocking bus names it: the configuration register"
        );
    }
}
