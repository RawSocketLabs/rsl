//! A batch of register writes: one `lms7002m_spi_post` call.

use hal::{FpgaAccess, Sequence, SequenceAction, SequenceFailure, SpiAddr};

use super::NAME;
use super::reg::Reg;
use super::spi;

/// The most writes one batch holds: `lms7002m_rfe_path`'s eleven.
pub const BATCH_CAPACITY: usize = 11;

/// Register writes to one chip, issued in order; the first failure abandons the rest, as
/// `lms7002m_spi_post` returns at its first failed exchange. Fixed capacity, no allocation.
///
/// Building one changes the [`Lms7002m`](crate::Lms7002m) cache already, so a batch that is
/// never run leaves the cache describing writes the chip did not get.
#[derive(Clone, Copy, Debug)]
#[must_use = "the chip cache already describes these writes: run the batch"]
pub struct Batch {
    /// The SPI target the chip answers on.
    target: SpiAddr,

    /// The writes; the first `len` are used.
    writes: [(Reg, u16); BATCH_CAPACITY],

    /// How many writes the batch holds.
    len: usize,

    /// Writes completed.
    done: usize,

    /// A write failed; nothing more is issued.
    failed: bool,
}

impl Batch {
    /// An empty batch for the chip at `target`.
    pub(crate) const fn new(target: SpiAddr) -> Self {
        Self {
            target,
            writes: [(Reg::LmlControl, 0); BATCH_CAPACITY],
            len: 0,
            done: 0,
            failed: false,
        }
    }

    /// Appends a write.
    ///
    /// # Panics
    ///
    /// Past [`BATCH_CAPACITY`]: every batch this crate builds has a fixed size within it.
    pub(crate) fn push(&mut self, write: (Reg, u16)) {
        assert!(
            self.len < BATCH_CAPACITY,
            "invariant: batches fit their capacity"
        );
        self.writes[self.len] = write;
        self.len += 1;
    }

    /// The writes, in issue order.
    #[must_use]
    pub fn writes(&self) -> &[(Reg, u16)] {
        &self.writes[..self.len]
    }

    /// The exchange to perform next.
    fn access(&self) -> Option<FpgaAccess> {
        if self.failed {
            return None;
        }
        let &write = self.writes().get(self.done)?;
        Some(spi::write(self.target, write))
    }
}

impl Sequence for Batch {
    fn next(&self) -> Option<SequenceAction> {
        self.access().map(SequenceAction::Fpga)
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(access) = self.access() else {
            return Ok(());
        };
        if let Err(source) = result {
            self.failed = true;
            return Err(SequenceFailure::FpgaTransport {
                chip: NAME,
                access,
                source,
            });
        }
        self.done += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use hal::Access;

    use super::*;

    #[test]
    fn a_failed_write_names_its_register_and_abandons_the_rest() {
        let mut batch = Batch::new(SpiAddr(0));
        batch.push((Reg::Afe, 1));
        batch.push((Reg::Xbuf, 2));
        let failure = batch.complete(Err(())).unwrap_err();
        assert_eq!(failure.access(), Some((NAME, Access::Write, 0x0082)));
        assert_eq!(batch.next(), None, "the write after it is not issued");
    }

    #[test]
    fn an_empty_batch_issues_nothing() {
        assert_eq!(Batch::new(SpiAddr(0)).next(), None);
    }
}
