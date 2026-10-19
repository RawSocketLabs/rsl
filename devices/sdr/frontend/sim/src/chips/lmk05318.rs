//! The LMK05318 network synchronizer of the `SimpleSync` and XMASS front ends
//! (`hw/lmk05318/lmk05318.c`): byte registers behind a 16-bit address, most significant
//! byte first (`lmk05318_reg_wr` sends `reg >> 8, reg`), auto-incrementing.
//!
//! The pinned `hw/lmk05318/lmk05318.yaml` documents fields but no reset values, so every
//! register resets to 0 except the identity and the two VCO status registers below.
//! Assumptions, each a sim choice the pinned source cannot settle (the `PE_SYNC` sim models
//! the same chip on the same evidence):
//! - Identity: addresses 0x0000-0x0003 hold `10 0b 35 42`, read as one four-byte burst;
//!   `lmk05318_create` refuses anything else (`-ENODEV`).
//! - `PLL1_CALSTAT1` (0x165) and `PLL2_CALSTAT1` (0x16f), YAML page "Status", which the C
//!   only reads: bit 5 `PLLn_VM_INSIDE` reports the charge pump in range, which the APLL1
//!   and APLL2 lock waits require. They ignore writes. Lock is reported from power-on
//!   (or never, [`Lmk05318::unlock`]), not acquired by calibration: libusdr's lock waits
//!   time out by wall clock, and a reported lock ends each on its first poll.
//! - No reset or sync side effects: `DEV_CTL` (0x0c) `RESET_SW` and `SYNC_SW` are stored
//!   like any other bit. The interrupt flags (`INT_FLAG0`/`INT_FLAG1`, 0x13/0x14) are plain
//!   storage, so after libusdr clears them they report no loss of lock or signal.
//! - `BAW_LOCKDET_PPM_MAX_BY1` (0x50) is plain storage. The YAML calls bit 7 `BAW_LOCK`
//!   "BAW Lock Detect Status", yet the C writes it (1 on its no-DPLL path) and gives no
//!   access mode; stored, it reads back the 1 the configuration wrote, which
//!   `lmk05318_wait_apll1_lock` requires. Whether hardware reports a live status there is
//!   open.
//! - DPLL reference validation (`REFVALSTAT`) is not modelled: both boards leave the DPLL
//!   disabled, so libusdr never reads it.

use super::I2cChip;
use super::reg16::Reg16File;

/// The identification bytes from address 0, as `lmk05318_create` requires them.
const ID: [(u16, u8); 4] = [(0, 0x10), (1, 0x0b), (2, 0x35), (3, 0x42)];
/// `PLL1_CALSTAT1` and `PLL2_CALSTAT1`.
const VCO_STATUS: [u16; 2] = [0x0165, 0x016f];
/// `PLLn_VM_INSIDE`, bit 5 of each VCO status register.
const VM_INSIDE: u8 = 1 << 5;

/// An LMK05318 whose APLLs report lock unless [`Lmk05318::unlock`]ed.
#[derive(Debug)]
pub(crate) struct Lmk05318 {
    /// Every register but the VCO status pair, which is computed.
    file: Reg16File,
    /// Whether the VCO status registers report the charge pumps in range.
    locked: bool,
}

impl Lmk05318 {
    /// A chip fresh from power-on, its APLLs reporting lock.
    pub(crate) fn new() -> Self {
        Self {
            file: Reg16File::with_resets(&ID),
            locked: true,
        }
    }

    /// Makes both APLLs report their charge pumps out of range from now on, so they never
    /// lock.
    pub(crate) fn unlock(&mut self) {
        self.locked = false;
    }

    /// The register at `addr`.
    pub(crate) fn reg(&self, addr: u16) -> u8 {
        if VCO_STATUS.contains(&addr) {
            return if self.locked { VM_INSIDE } else { 0 };
        }
        self.file.reg(addr)
    }
}

impl I2cChip for Lmk05318 {
    fn transfer(&mut self, write: &[u8], read_len: usize) -> Vec<u8> {
        let [high, low, data @ ..] = write else {
            return vec![0xff; read_len];
        };
        let mut addr = u16::from_be_bytes([*high, *low]);
        for &byte in data {
            if !VCO_STATUS.contains(&addr) {
                self.file.set(addr, byte);
            }
            addr = addr.wrapping_add(1);
        }
        (0..read_len)
            .map(|_| {
                let byte = self.reg(addr);
                addr = addr.wrapping_add(1);
                byte
            })
            .collect()
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
    fn vco_status_ignores_writes_and_reports_unlock_once_unlocked() {
        let mut chip = Lmk05318::new();
        chip.transfer(&[0x01, 0x65, 0x00], 0);
        assert_eq!(chip.reg(0x0165), VM_INSIDE);
        chip.unlock();
        assert_eq!(chip.transfer(&[0x01, 0x6f], 1), vec![0]);
    }

    #[test]
    fn other_registers_are_plain_storage() {
        let mut chip = Lmk05318::new();
        chip.transfer(&[0x00, 0x50, 0x80], 0);
        assert_eq!(chip.transfer(&[0x00, 0x50], 1), vec![0x80]);
    }
}
