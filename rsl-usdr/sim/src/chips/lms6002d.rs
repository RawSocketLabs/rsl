//! LMS6002D RF transceiver: 7-bit address, 8-bit data, over 16-bit SPI words.
//!
//! A plain register file, except the RX PLL's VTUNE comparator (register 0x2A), which
//! reports where the programmed VCO capacitor sits against a lock window; see
//! [`RxPllLock`].

/// Write flag in an SPI word.
const WRITE: u32 = 0x8000;

/// RX PLL fractional divider, bytes 0x20..=0x23 (`NINT` bits 31:23, `NFRAC` bits 22:0).
const RX_DIVIDER: usize = 0x20;
/// RX PLL VCO and divider selection (`FRANGE`, bits 4:2).
const RX_VCO_SELECT: usize = 0x25;
/// RX PLL VCO capacitor (`VCOCAP`, bits 5:0).
const RX_VCO_CAP: usize = 0x29;
/// RX PLL VTUNE comparator (`VTUNE_H` bit 7, `VTUNE_L` bit 6), read-only.
const RX_VTUNE: usize = 0x2a;

/// `VTUNE_H`: libusdr raises the capacitor when it reads this.
const VTUNE_H: u8 = 0x80;
/// `VTUNE_L`: libusdr lowers the capacitor when it reads this.
const VTUNE_L: u8 = 0x40;

/// The PLL reference: the board's 26 MHz clock. The sim models no external reference.
const REFERENCE_HZ: u64 = 26_000_000;

/// How the RX PLL's VTUNE comparator answers.
///
/// Assumed, not measured: Lime documents the comparator's two bits but not where a real
/// VCO locks, so the window is invented. It makes libusdr's capacitor search take a
/// realistic path (a binary search, then a linear scan across the window) instead of the
/// degenerate one an always-in-range comparator gives; it says nothing about whether
/// hardware would lock. The answer is a pure function of the registers, so traces are
/// deterministic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RxPllLock {
    /// The lowest capacitor code that locks.
    pub low: u8,

    /// The highest capacitor code that locks.
    pub high: u8,

    /// Below this LO frequency, in Hz, nothing locks: the comparator always reports the
    /// capacitor too high, so libusdr's search ends without a lock (`-ENOLCK`). 0 locks
    /// everywhere.
    pub unlocked_below_hz: u64,
}

impl Default for RxPllLock {
    /// Locks at capacitor codes 24 to 40 on every frequency.
    fn default() -> Self {
        Self {
            low: 24,
            high: 40,
            unlocked_below_hz: 0,
        }
    }
}

/// The LMS6002D register file.
#[derive(Debug)]
pub(crate) struct Lms6002d {
    /// Register contents, indexed by 7-bit address.
    pub(crate) regs: [u8; 128],

    /// How the RX PLL comparator answers.
    pub(crate) rx_lock: RxPllLock,
}

impl Lms6002d {
    /// A chip whose version register (0x04) reads 0x22, the `LMS6002Dr2` datasheet value.
    pub(crate) fn new() -> Self {
        let mut regs = [0; 128];
        regs[0x04] = 0x22;
        Self {
            regs,
            rx_lock: RxPllLock::default(),
        }
    }

    /// Performs one SPI word: `[15] write, [14:8] address, [7:0] data`. Returns the read
    /// data in the low byte (zero for writes).
    pub(crate) fn transact(&mut self, word: u32) -> u32 {
        let [data, addr, ..] = word.to_le_bytes();
        let addr = usize::from(addr & 0x7f);
        if word & WRITE != 0 {
            self.regs[addr] = data;
            return 0;
        }
        if addr == RX_VTUNE {
            return u32::from(self.rx_vtune());
        }
        u32::from(self.regs[addr])
    }

    /// The RX comparator's bits for the programmed capacitor and LO.
    fn rx_vtune(&self) -> u8 {
        if self.rx_lo_hz() < self.rx_lock.unlocked_below_hz {
            return VTUNE_L;
        }
        let cap = self.regs[RX_VCO_CAP] & 0x3f;
        if cap < self.rx_lock.low {
            VTUNE_H
        } else if cap > self.rx_lock.high {
            VTUNE_L
        } else {
            0
        }
    }

    /// The RX LO the PLL registers program: the VCO, `26 MHz × (NINT + NFRAC / 2^23)`,
    /// divided by `2^(FRANGE[1:0] + 1)`.
    fn rx_lo_hz(&self) -> u64 {
        let divider = u64::from(u32::from_be_bytes([
            self.regs[RX_DIVIDER],
            self.regs[RX_DIVIDER + 1],
            self.regs[RX_DIVIDER + 2],
            self.regs[RX_DIVIDER + 3],
        ]));
        // NINT and NFRAC together are the divider in 2^-23 units.
        let vco_hz = (REFERENCE_HZ * divider) >> 23;
        let range = (self.regs[RX_VCO_SELECT] >> 2) & 0b11;
        vco_hz >> (range + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A chip with the RX PLL programmed as libusdr does for 1 GHz (NINT 153, NFRAC
    /// 7098052, VCO4, divider range 1) and capacitor `cap`.
    fn tuned_to_1ghz(cap: u8) -> Lms6002d {
        let mut chip = Lms6002d::new();
        for word in [
            0xa04c,
            0xa1ec,
            0xa24e,
            0xa3c4,
            0xa595,
            0xa900 | u32::from(cap),
        ] {
            chip.transact(word);
        }
        chip
    }

    #[test]
    fn programmed_lo_is_decoded_from_the_divider() {
        assert_eq!(tuned_to_1ghz(32).rx_lo_hz() / 1000, 999_999);
    }

    #[test]
    fn comparator_brackets_the_lock_window() {
        let read = |cap| tuned_to_1ghz(cap).transact(0x2a00);
        assert_eq!(read(23), u32::from(VTUNE_H), "below the window: raise");
        assert_eq!(read(24), 0);
        assert_eq!(read(40), 0);
        assert_eq!(read(41), u32::from(VTUNE_L), "above the window: lower");
    }

    #[test]
    fn below_the_unlocked_frequency_nothing_locks() {
        let mut chip = tuned_to_1ghz(32);
        chip.rx_lock.unlocked_below_hz = 2_000_000_000;
        assert_eq!(chip.transact(0x2a00), u32::from(VTUNE_L));
    }
}
