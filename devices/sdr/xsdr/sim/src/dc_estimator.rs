//! The RX PHY's DC estimator (`PHY_REG_RXBANK_DC_EST`, bank 6 of `REG_CFG_PHY_0`), as
//! libusdr's RF calibration and DC readback use it (`device/m2_lm7_1/xsdr_ctrl.c`:
//! `xsdr_phy_dc_estim_*`, `xsdr_rfe_pwrdc_get`, `xsdrcal_do_meas_nco_avg`).
//!
//! Addressing, from `xsdr_phy_rx_reg`: `bit 31 write | bank 30:24 | address 23:16 |
//! value 15:0`. Written, address 0 (`ADDR_DC_EST_RESET`) holds the estimator (1) or runs it
//! (0) and address 1 (`ADDR_DC_EST_ACCM`) sets its accumulation; with bit 31 clear, the
//! address selects the word the next read of the port returns: the generation (0) or a
//! channel's I or Q (4 to 7: A I, A Q, B I, B Q; `enum dc_estimations`).
//!
//! Assumed, not documented (no gateware source is vendored), as in `usdr-sim`'s estimator
//! (whose gateware block libusdr drives the same way, `ipblks/dc_estim.c`):
//! - The estimator finishes one measurement per millisecond of virtual time while it
//!   runs, counting them in its generation word, whatever the accumulation; holding it
//!   keeps the count. libusdr polls the generation every 500 µs, so the second poll after a
//!   start sees it move.
//! - Its I and Q words are the receive output's mean at DC (`lms7002m-sim`'s
//!   `rx_mean`, in 12-bit LSBs) less the half LSB libusdr adds back ("Static correction
//!   by +0.5 bits in FPGA"), times 65536 and the accumulation (libusdr divides by both),
//!   rounded to nearest and saturated to 32 bits. They are computed when read; libusdr
//!   holds the chip still while a measurement runs.
//!
//! The estimator is small and its gateware shared, but its words read different chips'
//! models here and in `usdr-sim`, so each sim keeps its own copy of the timing.

use lms7002m_sim::Lms7002m;

/// The DC estimator's bank on the RX PHY port.
pub(crate) const BANK: u32 = 6;
/// Bit 31 of a port write: a write (`xsdr_phy_rx_reg`'s `wr`).
const WRITE: u32 = 1 << 31;
/// Write address: hold (1) or run (0) (`ADDR_DC_EST_RESET`).
const HOLD: u8 = 0;
/// Write address: the accumulation (`ADDR_DC_EST_ACCM`).
const ACCUMULATION: u8 = 1;
/// Read address: the generation (`DC_ESTIM_GEN`).
const GENERATION: u8 = 0;
/// Virtual microseconds per measurement.
const MEASUREMENT_US: u64 = 1000;

/// The estimator's state.
#[derive(Debug, Default)]
pub(crate) struct DcEstimator {
    /// When it last started running, in virtual microseconds; `None` while held.
    started_us: Option<u64>,
    /// The generation it had when it last started or was held.
    base_generation: u32,
    /// The accumulation last written.
    accumulation: u16,
    /// The word the next read of the port returns, once one was selected.
    selected: Option<u8>,
}

impl DcEstimator {
    /// The generation at `now_us`.
    fn generation(&self, now_us: u64) -> u32 {
        let finished = self
            .started_us
            .map_or(0, |start| now_us.saturating_sub(start) / MEASUREMENT_US);
        self.base_generation
            .wrapping_add(u32::try_from(finished).unwrap_or(u32::MAX))
    }

    /// Follows a write of `value` to the RX PHY port at `now_us`; any other bank's
    /// request deselects the estimator.
    pub(crate) fn on_write(&mut self, value: u32, now_us: u64) {
        if value >> 24 & 0x7f != BANK {
            self.selected = None;
            return;
        }
        let [_, _, address, _] = value.to_le_bytes();
        if value & WRITE == 0 {
            self.selected = Some(address);
            return;
        }
        let [low, high, ..] = value.to_le_bytes();
        match address {
            HOLD => {
                self.base_generation = self.generation(now_us);
                self.started_us = (low & 1 == 0).then_some(now_us);
            }
            ACCUMULATION => self.accumulation = u16::from_le_bytes([low, high]),
            _ => {}
        }
    }

    /// What a read of the port returns at `now_us` while a word is selected.
    pub(crate) fn read(&self, now_us: u64, lms: &Lms7002m) -> Option<u32> {
        let selected = self.selected?;
        if selected == GENERATION {
            return Some(self.generation(now_us));
        }
        let (channel, part) = match selected {
            4 => (0, 0),
            5 => (0, 1),
            6 => (1, 0),
            7 => (1, 1),
            _ => return Some(0),
        };
        let mean = lms.rx_mean(channel)[part];
        let scaled = (mean - 0.5) * 65_536.0 * f64::from(self.accumulation);
        #[expect(
            clippy::cast_possible_truncation,
            reason = "rounded and clamped to the i32 range first"
        )]
        let word = scaled
            .round()
            .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32;
        Some(u32::from_ne_bytes(word.to_ne_bytes()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A port write: `write`, bank 6, `address`, `value`.
    const fn access(write: bool, address: u32, value: u32) -> u32 {
        (if write { WRITE } else { 0 }) | BANK << 24 | address << 16 | value
    }

    #[test]
    fn the_estimator_counts_a_measurement_per_millisecond_while_running() {
        let lms = Lms7002m::new(0);
        let mut estimator = DcEstimator::default();
        let read = |estimator: &mut DcEstimator, at| {
            estimator.on_write(access(false, 0, 0), at);
            estimator.read(at, &lms)
        };
        estimator.on_write(access(true, 0, 1), 0);
        estimator.on_write(access(true, 0, 0), 1);
        assert_eq!(read(&mut estimator, 1), Some(0));
        assert_eq!(read(&mut estimator, 1000), Some(0));
        assert_eq!(read(&mut estimator, 1001), Some(1));
        estimator.on_write(access(true, 0, 1), 5000);
        assert_eq!(read(&mut estimator, 9000), Some(4), "held");
    }

    #[test]
    fn a_silent_receiver_reads_minus_half_an_lsb_per_accumulation() {
        let lms = Lms7002m::new(0);
        let mut estimator = DcEstimator::default();
        estimator.on_write(access(true, 1, 3), 0);
        estimator.on_write(access(false, 6, 0), 0);
        let word = estimator
            .read(0, &lms)
            .map(|w| i32::from_ne_bytes(w.to_ne_bytes()));
        assert_eq!(word, Some(-3 * 32_768), "channel B's I");
        estimator.on_write(1 << 31 | 5 << 24, 0);
        assert_eq!(estimator.read(0, &lms), None, "another bank deselects it");
    }
}
