//! LMS6002D RF transceiver: 7-bit address, 8-bit data, over 16-bit SPI words.
//!
//! A plain register file, except where libusdr reads back something the chip computes:
//! each synthesizer's VTUNE comparator (registers 0x1A and 0x2A), which reports where the
//! programmed VCO capacitor sits against a lock window ([`PllLock`]), and the DC-offset
//! calibration engines, which report busy for a while after a start and then a result
//! ([`DcCalibration`]).

/// Write flag in an SPI word.
const WRITE: u32 = 0x8000;

/// The TX synthesizer's first register (`TxPLL`, 0x10-0x1F).
const TX_PLL: usize = 0x10;
/// The RX synthesizer's first register (`RxPLL`, 0x20-0x2F).
const RX_PLL: usize = 0x20;
/// Within a synthesizer: the fractional divider, bytes 0..=3 (`NINT` bits 31:23, `NFRAC`
/// bits 22:0).
const PLL_DIVIDER: usize = 0x0;
/// Within a synthesizer: VCO and divider selection (`FRANGE`, bits 4:2).
const PLL_VCO_SELECT: usize = 0x5;
/// Within a synthesizer: the VCO capacitor (`VCOCAP`, bits 5:0).
const PLL_VCO_CAP: usize = 0x9;
/// Within a synthesizer: the VTUNE comparator, read-only.
const PLL_VTUNE: usize = 0xa;

/// `VTUNE_H`/`COMPH`: libusdr raises the capacitor when it reads this.
const VTUNE_H: u8 = 0x80;
/// `VTUNE_L`/`COMPL`: libusdr lowers the capacitor when it reads this.
const VTUNE_L: u8 = 0x40;

/// The blocks with a DC-offset calibration engine: top-level LPF tuning, TX LPF, RX LPF,
/// RXVGA2 (libusdr's `calibration_loop` bases).
const DC_BLOCKS: [usize; 4] = [0x00, 0x30, 0x50, 0x60];
/// Within a calibration block: the result (`DC_REGVAL`, bits 5:0; libusdr reads 4:0).
const DC_REG: usize = 0x0;
/// Within a calibration block: status, read-only (`DC_CLBR_DONE`, bit 1: still running).
const DC_CAL: usize = 0x1;
/// Within a calibration block: control (`DC_START_CLBR`, bit 5; `DC_ADDR`, bits 2:0).
const DC_OP: usize = 0x3;
/// `DC_CLBR_DONE`: the calibration is still running.
const CLBR_DONE: u8 = 0x02;
/// `DC_START_CLBR`.
const START_CLBR: u8 = 0x20;

/// The PLL reference: the board's 26 MHz clock. The sim models no external reference.
const REFERENCE_HZ: u64 = 26_000_000;

/// How a synthesizer's VTUNE comparator answers.
///
/// Assumed, not measured: Lime documents the comparator's two bits but not where a real
/// VCO locks, so the window is invented. It makes libusdr's capacitor search take a
/// realistic path (a binary search, then a linear scan across the window) instead of the
/// degenerate one an always-in-range comparator gives; it says nothing about whether
/// hardware would lock. The answer is a pure function of the registers, so traces are
/// deterministic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PllLock {
    /// The lowest capacitor code that locks.
    pub low: u8,

    /// The highest capacitor code that locks.
    pub high: u8,

    /// Below this LO frequency, in Hz, nothing locks: the comparator always reports the
    /// capacitor too high, so libusdr's search ends without a lock (`-ENOLCK`). 0 locks
    /// everywhere.
    pub unlocked_below_hz: u64,
}

impl Default for PllLock {
    /// Locks at capacitor codes 24 to 40 on every frequency.
    fn default() -> Self {
        Self {
            low: 24,
            high: 40,
            unlocked_below_hz: 0,
        }
    }
}

/// How the DC-offset calibration engines answer.
///
/// Assumed, not measured: after a start, `DC_CLBR_DONE` reads busy for `busy_polls` reads,
/// then `DC_REG` reads `result` for every block and channel. Lime documents the bits, not
/// the timing or the values a chip converges to. A `result` of 0x1F (saturated) makes
/// libusdr retry until it gives up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DcCalibration {
    /// Status reads that report the calibration still running.
    pub busy_polls: u8,

    /// The calibrated value, five bits.
    pub result: u8,
}

impl Default for DcCalibration {
    /// Busy for two reads, then 0x10.
    fn default() -> Self {
        Self {
            busy_polls: 2,
            result: 0x10,
        }
    }
}

/// The LMS6002D register file.
#[derive(Debug)]
pub(crate) struct Lms6002d {
    /// Register contents, indexed by 7-bit address.
    pub(crate) regs: [u8; 128],

    /// How the RX synthesizer's comparator answers.
    pub(crate) rx_lock: PllLock,

    /// How the TX synthesizer's comparator answers; libusdr tunes TX only to calibrate.
    pub(crate) tx_lock: PllLock,

    /// How the DC calibration engines answer.
    pub(crate) dc_calibration: DcCalibration,

    /// Status reads left before each calibration block finishes, by [`DC_BLOCKS`] index.
    dc_busy: [u8; 4],

    /// Each calibration block has been started, by [`DC_BLOCKS`] index.
    dc_started: [bool; 4],
}

impl Lms6002d {
    /// A chip whose version register (0x04) reads 0x22, the `LMS6002Dr2` datasheet value.
    pub(crate) fn new() -> Self {
        let mut regs = [0; 128];
        regs[0x04] = 0x22;
        Self {
            regs,
            rx_lock: PllLock::default(),
            tx_lock: PllLock::default(),
            dc_calibration: DcCalibration::default(),
            dc_busy: [0; 4],
            dc_started: [false; 4],
        }
    }

    /// Performs one SPI word: `[15] write, [14:8] address, [7:0] data`. Returns the read
    /// data in the low byte (zero for writes).
    pub(crate) fn transact(&mut self, word: u32) -> u32 {
        let [data, addr, ..] = word.to_le_bytes();
        let addr = usize::from(addr & 0x7f);
        if word & WRITE != 0 {
            self.write(addr, data);
            return 0;
        }
        u32::from(self.read(addr))
    }

    /// Stores a register, starting a calibration on a `DC_START_CLBR` write.
    fn write(&mut self, addr: usize, data: u8) {
        self.regs[addr] = data;
        if let Some(block) = dc_block(addr, DC_OP) {
            if data & START_CLBR != 0 {
                self.dc_busy[block] = self.dc_calibration.busy_polls;
                self.dc_started[block] = true;
            }
        }
    }

    /// A register as the chip reports it.
    fn read(&mut self, addr: usize) -> u8 {
        if addr == TX_PLL + PLL_VTUNE {
            return self.vtune(TX_PLL, self.tx_lock);
        }
        if addr == RX_PLL + PLL_VTUNE {
            return self.vtune(RX_PLL, self.rx_lock);
        }
        if let Some(block) = dc_block(addr, DC_CAL) {
            if self.dc_busy[block] > 0 {
                self.dc_busy[block] -= 1;
                return CLBR_DONE;
            }
            return 0;
        }
        if let Some(block) = dc_block(addr, DC_REG) {
            // Only once a calibration ran; before that the register reads as written.
            if self.dc_started[block] {
                return self.dc_calibration.result & 0x3f;
            }
        }
        self.regs[addr]
    }

    /// The comparator bits of the synthesizer at `base` for its programmed capacitor and LO.
    fn vtune(&self, base: usize, lock: PllLock) -> u8 {
        if self.lo_hz(base) < lock.unlocked_below_hz {
            return VTUNE_L;
        }
        let cap = self.regs[base + PLL_VCO_CAP] & 0x3f;
        if cap < lock.low {
            VTUNE_H
        } else if cap > lock.high {
            VTUNE_L
        } else {
            0
        }
    }

    /// The LO the synthesizer at `base` is programmed for: the VCO,
    /// `26 MHz × (NINT + NFRAC / 2^23)`, divided by `2^(FRANGE[1:0] + 1)`.
    fn lo_hz(&self, base: usize) -> u64 {
        let divider = base + PLL_DIVIDER;
        let divider = u64::from(u32::from_be_bytes([
            self.regs[divider],
            self.regs[divider + 1],
            self.regs[divider + 2],
            self.regs[divider + 3],
        ]));
        // NINT and NFRAC together are the divider in 2^-23 units.
        let vco_hz = (REFERENCE_HZ * divider) >> 23;
        let range = (self.regs[base + PLL_VCO_SELECT] >> 2) & 0b11;
        vco_hz >> (range + 1)
    }
}

/// The [`DC_BLOCKS`] index of the calibration block whose register `offset` is `addr`.
fn dc_block(addr: usize, offset: usize) -> Option<usize> {
    DC_BLOCKS.iter().position(|&base| base + offset == addr)
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
        assert_eq!(tuned_to_1ghz(32).lo_hz(RX_PLL) / 1000, 999_999);
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

    #[test]
    fn a_started_calibration_is_busy_then_reports_its_result() {
        let mut chip = Lms6002d::new();
        // libusdr's start sequence on the RX LPF block (0x50), channel 1.
        for word in [0xd01f, 0xd309, 0xd329, 0xd309] {
            chip.transact(word);
        }
        let status = |chip: &mut Lms6002d| chip.transact(0x5100);
        assert_eq!(status(&mut chip), u32::from(CLBR_DONE));
        assert_eq!(status(&mut chip), u32::from(CLBR_DONE));
        assert_eq!(status(&mut chip), 0, "done after two busy reads");
        assert_eq!(
            chip.transact(0x5000),
            0x10,
            "the calibrated value, not the 31 written"
        );
    }
}
