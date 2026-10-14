//! A lock wait: read a status register until it reports lock, sleeping between reads, as
//! the TI chips' `*_wait_pll_lock` functions do.
//!
//! libusdr times these waits by wall clock (`clock_get_time`); this counts the sleeps
//! between reads instead, so on hardware it waits at least as long, and on a simulator the
//! trace does not depend on the host's speed.

use core::time::Duration;

use hal::{Sequence, SequenceAction, SequenceFailure};

use super::outcome::outcome;
use super::ti_spi;

/// How a lock wait ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Wait {
    /// Still reading.
    Pending,
    /// The chip reported lock.
    Locked,
    /// It did not within the timeout.
    TimedOut,
    /// A read failed; the sequence reported the failure.
    Failed,
}

/// Reads `read` until `locked` holds for the register value, sleeping `interval` after
/// each read that does not, for at most `timeout` of sleeps.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LockPoll {
    /// The chip, for failure reports.
    chip: &'static str,
    /// The status read.
    read: SequenceAction,
    /// Whether a status value reports lock.
    locked: fn(u16) -> bool,
    /// The sleep after an unlocked read.
    interval: Duration,
    /// How many unlocked reads end the wait: the timeout over the interval.
    max_reads: u32,
    /// Unlocked reads so far.
    reads: u32,
    /// Sleeping after an unlocked read, rather than reading.
    sleeping: bool,
    /// How the wait ended, if it has.
    outcome: Wait,
}

impl LockPoll {
    /// A wait on `read`, polled every `interval` for at most `timeout`.
    ///
    /// # Panics
    ///
    /// If `interval` is zero or `timeout` holds more than `u32::MAX` intervals: the chips'
    /// waits are fixed, small multiples.
    pub(crate) fn new(
        chip: &'static str,
        read: SequenceAction,
        locked: fn(u16) -> bool,
        interval: Duration,
        timeout: Duration,
    ) -> Self {
        let max_reads = u32::try_from(timeout.as_nanos() / interval.as_nanos())
            .expect("invariant: lock waits are a fixed, small number of polls");
        Self {
            chip,
            read,
            locked,
            interval,
            max_reads,
            reads: 0,
            sleeping: false,
            outcome: Wait::Pending,
        }
    }

    /// How the wait ended, or [`Wait::Pending`].
    pub(crate) const fn outcome(&self) -> Wait {
        self.outcome
    }
}

impl Sequence for LockPoll {
    fn next(&self) -> Option<SequenceAction> {
        if self.outcome != Wait::Pending {
            return None;
        }
        Some(if self.sleeping {
            SequenceAction::Sleep(self.interval)
        } else {
            self.read
        })
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(action) = self.next() else {
            return Ok(());
        };
        if self.sleeping {
            self.sleeping = false;
            self.reads += 1;
            if self.reads >= self.max_reads {
                self.outcome = Wait::TimedOut;
            }
            return Ok(());
        }
        // A failed read ends the wait, as libusdr's `return res` does.
        let word =
            outcome(action, result, self.chip).inspect_err(|_| self.outcome = Wait::Failed)?;
        if (self.locked)(ti_spi::value(word)) {
            self.outcome = Wait::Locked;
        } else {
            self.sleeping = true;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use hal::{FpgaAccess, SpiAddr};

    use super::*;

    const READ: SequenceAction = ti_spi::read(SpiAddr(2), 0x4a);

    fn poll() -> LockPoll {
        LockPoll::new(
            "LMX2820",
            READ,
            |value| value >> 14 == 2,
            Duration::from_micros(100),
            Duration::from_millis(1),
        )
    }

    #[test]
    fn a_lock_on_the_first_read_ends_the_wait_at_once() {
        let mut wait = poll();
        assert_eq!(wait.next(), Some(READ));
        wait.complete::<()>(Ok(0x8000)).unwrap();
        assert_eq!((wait.next(), wait.outcome()), (None, Wait::Locked));
    }

    #[test]
    fn the_timeout_counts_the_sleeps_between_reads() {
        let mut wait = poll();
        let mut reads = 0;
        while let Some(action) = wait.next() {
            if matches!(action, SequenceAction::Fpga(FpgaAccess::Spi { .. })) {
                reads += 1;
            }
            wait.complete::<()>(Ok(0)).unwrap();
        }
        assert_eq!(
            (reads, wait.outcome()),
            (10, Wait::TimedOut),
            "1 ms of 100 µs sleeps"
        );
    }

    #[test]
    fn a_failed_read_ends_the_wait_with_the_failure() {
        let mut wait = poll();
        assert!(wait.complete(Err(())).is_err());
        assert_eq!((wait.next(), wait.outcome()), (None, Wait::Failed));
    }
}
