//! The LMK05318 network synchronizer (`hw/lmk05318/lmk05318.c`) as a register model, shared
//! by the simulated boards that carry the chip: the `SimpleSync` and XMASS front ends
//! (`sdr-frontend-sim`) and `PE_SYNC` (`pe-sync-sim`). Byte registers behind a 16-bit
//! pointer, most significant byte first (`lmk05318_reg_wr` sends `reg >> 8, reg`), that
//! auto-increments past each byte written or read.
//!
//! The pinned `hw/lmk05318/lmk05318.yaml` documents fields but no reset values or access
//! modes, so every register resets to 0 and is plain storage except those below.
//! Assumptions, each a sim choice the pinned source cannot settle:
//! - Identity: addresses 0x0000-0x0003 hold `10 0b 35 42`, read as one four-byte burst;
//!   `lmk05318_create` refuses anything else (`-ENODEV`). They ignore writes (libusdr
//!   writes none).
//! - `PLL1_CALSTAT1` (0x165) and `PLL2_CALSTAT1` (0x16f), YAML page "Status", which the C
//!   only reads: bit 5 `PLLn_VM_INSIDE` reports the charge pump in range, which the APLL1
//!   and APLL2 lock waits require. They ignore writes. Lock is reported, not acquired by
//!   calibration: from power-on by default, so a lock wait ends on its first poll; per
//!   APLL, after a number of reads or never ([`Lmk05318::set_lock`], [`Lmk05318::unlock`]),
//!   to drive a wait's repeated polls or its timeout. Unlocked reads are counted per APLL
//!   across waits and opens, as a PLL that stays unlocked would.
//! - No reset or sync side effects: `DEV_CTL` (0x0c) `RESET_SW` and `SYNC_SW` are stored
//!   like any other bit. The interrupt flags (`INT_FLAG0`/`INT_FLAG1`, 0x13/0x14) are plain
//!   storage, so after libusdr clears them they report no loss of lock or signal.
//! - `BAW_LOCKDET_PPM_MAX_BY1` (0x50) is plain storage. The YAML calls bit 7 `BAW_LOCK`
//!   "BAW Lock Detect Status", yet the C writes it (1 on its no-DPLL path,
//!   `no_dpll_regs`; 0 on the DPLL path); stored, it reads back the 1 the configuration
//!   wrote, which `lmk05318_wait_apll1_lock` requires. Whether hardware reports a live
//!   status there is open.
//! - DPLL reference validation (`REFVALSTAT`) is not modelled: every board libusdr drives
//!   leaves the DPLL disabled, so `lmk05318_wait_dpll_ref_stat` never reads it.
//! - A transfer without a whole pointer reads the idle bus (`0xff`); libusdr always sends
//!   one.
#![forbid(unsafe_code)]

/// The identification bytes from address 0, as `lmk05318_create` requires them.
const ID: [u8; 4] = [0x10, 0x0b, 0x35, 0x42];
/// `PLL1_CALSTAT1` and `PLL2_CALSTAT1`.
const VCO_STATUS: [u16; 2] = [0x0165, 0x016f];
/// `PLLn_VM_INSIDE`, bit 5 of each VCO status register.
const VM_INSIDE: u8 = 1 << 5;

/// One of the chip's two APLLs, by its VCO status register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Apll {
    /// APLL1 (`PLL1_CALSTAT1`).
    Apll1,
    /// APLL2 (`PLL2_CALSTAT1`).
    Apll2,
}

/// How a PLL's lock detect reports.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lock {
    /// Locked from the first read: a healthy chip.
    #[default]
    Immediate,
    /// The first `n` reads report unlocked, every later one locked: a slow lock.
    AfterReads(u32),
    /// Every read reports unlocked: a PLL that never locks.
    Never,
}

/// An LMK05318 whose APLLs report lock unless told otherwise ([`Lmk05318::set_lock`]).
#[derive(Debug)]
pub struct Lmk05318 {
    /// Every register's contents, indexed by address; the identity and VCO status
    /// registers are answered without it.
    regs: Vec<u8>,
    /// How each APLL's VCO status reports lock, by [`VCO_STATUS`] index.
    locks: [Lock; 2],
    /// Unlocked reads of each VCO status so far.
    unlocked_reads: [u32; 2],
}

impl Lmk05318 {
    /// A chip fresh from power-on, its APLLs reporting lock.
    #[must_use]
    pub fn new() -> Self {
        Self {
            regs: vec![0; 1 << 16],
            locks: [Lock::Immediate; 2],
            unlocked_reads: [0; 2],
        }
    }

    /// Makes `apll` report its lock as `lock` from now on, its unlocked reads counted
    /// afresh.
    pub fn set_lock(&mut self, apll: Apll, lock: Lock) {
        let i = apll as usize;
        self.locks[i] = lock;
        self.unlocked_reads[i] = 0;
    }

    /// Makes both APLLs report their charge pumps out of range from now on, so they never
    /// lock.
    pub fn unlock(&mut self) {
        self.set_lock(Apll::Apll1, Lock::Never);
        self.set_lock(Apll::Apll2, Lock::Never);
    }

    /// Whether the APLL with VCO status `VCO_STATUS[i]` reports lock now.
    fn locked(&self, i: usize) -> bool {
        match self.locks[i] {
            Lock::Immediate => true,
            Lock::AfterReads(n) => self.unlocked_reads[i] >= n,
            Lock::Never => false,
        }
    }

    /// The register at `addr`, as a read returns it.
    #[must_use]
    pub fn reg(&self, addr: u16) -> u8 {
        if let Some(&id) = ID.get(usize::from(addr)) {
            return id;
        }
        if let Some(i) = VCO_STATUS.iter().position(|&status| status == addr) {
            return if self.locked(i) { VM_INSIDE } else { 0 };
        }
        self.regs[usize::from(addr)]
    }

    /// One read of the register at `addr` over the bus, which counts towards a slow lock.
    fn read(&mut self, addr: u16) -> u8 {
        let byte = self.reg(addr);
        if let Some(i) = VCO_STATUS.iter().position(|&status| status == addr) {
            if byte == 0 {
                self.unlocked_reads[i] = self.unlocked_reads[i].saturating_add(1);
            }
        }
        byte
    }

    /// Performs a write-then-read transfer: `write` is the bytes sent (the two pointer
    /// bytes first, then data), and the returned bytes are what the chip transmits for
    /// `read_len` bytes, continuing from where the written data left the pointer.
    pub fn transfer(&mut self, write: &[u8], read_len: usize) -> Vec<u8> {
        let [high, low, data @ ..] = write else {
            return vec![0xff; read_len];
        };
        let mut addr = u16::from_be_bytes([*high, *low]);
        for &byte in data {
            if usize::from(addr) >= ID.len() && !VCO_STATUS.contains(&addr) {
                self.regs[usize::from(addr)] = byte;
            }
            addr = addr.wrapping_add(1);
        }
        (0..read_len)
            .map(|_| {
                let byte = self.read(addr);
                addr = addr.wrapping_add(1);
                byte
            })
            .collect()
    }
}

impl Default for Lmk05318 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_identity_reads_as_one_burst_and_the_vcos_report_lock() {
        let mut chip = Lmk05318::new();
        assert_eq!(chip.transfer(&[0, 0], 4), vec![0x10, 0x0b, 0x35, 0x42]);
        assert_eq!(chip.transfer(&[0x01, 0x65], 1), vec![VM_INSIDE]);
        assert_eq!(chip.transfer(&[0x01, 0x6f], 1), vec![VM_INSIDE]);
    }

    #[test]
    fn identity_and_vco_status_ignore_writes_and_unlock_clears_the_status() {
        let mut chip = Lmk05318::new();
        chip.transfer(&[0x00, 0x00, 0xaa], 0);
        chip.transfer(&[0x01, 0x65, 0x00], 0);
        assert_eq!((chip.reg(0), chip.reg(0x0165)), (0x10, VM_INSIDE));
        chip.unlock();
        assert_eq!(chip.transfer(&[0x01, 0x6f], 1), vec![0]);
    }

    #[test]
    fn a_slow_apll_reports_unlocked_for_its_reads_only() {
        let mut chip = Lmk05318::new();
        chip.set_lock(Apll::Apll2, Lock::AfterReads(2));
        let reads: Vec<u8> = (0..3)
            .flat_map(|_| chip.transfer(&[0x01, 0x6f], 1))
            .collect();
        assert_eq!(reads, [0, 0, VM_INSIDE]);
        assert_eq!(
            chip.transfer(&[0x01, 0x65], 1),
            vec![VM_INSIDE],
            "APLL1 unaffected"
        );
    }

    #[test]
    fn other_registers_are_plain_auto_incrementing_storage() {
        let mut chip = Lmk05318::new();
        chip.transfer(&[0x00, 0x50, 0x80, 0x81], 0);
        assert_eq!(chip.transfer(&[0x00, 0x50], 3), vec![0x80, 0x81, 0x00]);
    }

    #[test]
    fn a_short_pointer_reads_the_idle_bus() {
        assert_eq!(Lmk05318::new().transfer(&[0x00], 1), vec![0xff]);
    }
}
