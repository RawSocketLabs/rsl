//! A PLL's lock indication: read-only status bits that report lock after a chosen number
//! of reads, or never, so a driver's lock wait can be made to repeat or time out.
//!
//! Assumption, a sim choice: lock is counted in reads of the status register, not modelled
//! as acquired by calibration. libusdr's waits read the register once per poll, so the
//! count is the number of unlocked polls; it carries on across waits and opens, as a PLL
//! that stays unlocked would.

/// How a PLL's lock detect reports.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lock {
    /// Locked from the first read: a healthy board.
    #[default]
    Immediate,
    /// The first `n` reads report unlocked, every later one locked: a slow lock.
    AfterReads(u32),
    /// Every read reports unlocked: a PLL that never locks.
    Never,
}

/// Read-only bits of one register reporting a PLL's lock, as [`Lock`] says.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LockDetect {
    /// The status register.
    pub(crate) addr: u16,
    /// Its lock detect bits.
    mask: u16,
    /// Those bits when locked; when unlocked they read their reset value.
    locked: u16,
    /// How the lock reports.
    lock: Lock,
    /// Unlocked reads so far.
    reads: u32,
}

impl LockDetect {
    /// Lock detect bits `mask` of register `addr`, reading `locked` once locked.
    pub(crate) const fn new(addr: u16, mask: u16, locked: u16) -> Self {
        Self {
            addr,
            mask,
            locked,
            lock: Lock::Immediate,
            reads: 0,
        }
    }

    /// Reports as `lock` from now on, the unlocked reads counted afresh.
    pub(crate) fn set(&mut self, lock: Lock) {
        self.lock = lock;
        self.reads = 0;
    }

    /// One read of the status register holding `stored`: the value the chip reports.
    pub(crate) fn read(&mut self, stored: u16) -> u16 {
        let locked = match self.lock {
            Lock::Immediate => true,
            Lock::AfterReads(n) => self.reads >= n,
            Lock::Never => false,
        };
        if locked {
            return stored & !self.mask | self.locked & self.mask;
        }
        self.reads += 1;
        stored
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detect(lock: Lock) -> LockDetect {
        let mut detect = LockDetect::new(0x4a, 0xc000, 0x8000);
        detect.set(lock);
        detect
    }

    #[test]
    fn a_slow_lock_reports_unlocked_for_its_reads_then_locked_for_good() {
        let mut detect = detect(Lock::AfterReads(2));
        let reads: Vec<u16> = (0..4).map(|_| detect.read(0x0012)).collect();
        assert_eq!(reads, [0x0012, 0x0012, 0x8012, 0x8012]);
    }

    #[test]
    fn a_pll_that_never_locks_keeps_its_reset_bits() {
        let mut detect = detect(Lock::Never);
        assert!((0..1000).all(|_| detect.read(0xc000) == 0xc000));
    }
}
