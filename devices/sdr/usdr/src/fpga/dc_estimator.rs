//! The RX chain's DC estimator: the mean of a channel's samples after its NCO, which
//! libusdr's LO-leakage and IQ-imbalance calibration measures power with.
//!
//! It sits behind the RX configuration port ([`Phy::Rx`], `REG_CFG_PHY_0`) as bank 6
//! (`PHY_REG_RXBANK_DC_EST`), reached by words of its own layout ([`EstimatorAccess`]): a
//! write sets one of its registers; a read request selects the word the next read of the
//! port returns.
//!
//! Source: `ipblks/dc_estim.c` (`_phy_rx_reg`, `phy_dc_estim_*`, `phy_rfe_pwrdc_get`,
//! `phy_do_meas_nco_avg`).

use std::time::Duration;

use bnb::{bitfield, u7};

use super::gpio::FPGA;
use super::phy::Phy;
use crate::error::{BusContext, Error};
use crate::lowlevel::Bus;

/// One access to the estimator through the RX port (`_phy_rx_reg`).
#[bitfield(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EstimatorAccess {
    /// Select the word at `addr` for the next read rather than write it.
    #[bits(31..=31)]
    read: bool,

    /// The bank: always the estimator's.
    #[bits(24..=30)]
    bank: u7,

    /// The estimator register.
    #[bits(16..=23)]
    addr: u8,

    /// The value written; 0 for a read.
    #[bits(0..=15)]
    value: u16,
}

/// The estimator's bank on the RX port (`PHY_REG_RXBANK_DC_EST`).
const BANK: u8 = 6;

/// Write address: 1 holds the estimator, 0 runs it (`ADDR_DC_EST_RESET`).
const HOLD: u8 = 0;

/// Write address: the accumulation setting (`ADDR_DC_EST_ACCM`).
const ACCUMULATION: u8 = 1;

/// Read address: the generation, which moves when a measurement finishes
/// (`DC_ESTIM_GEN`).
const GENERATION: u8 = 0;

/// Read addresses of channel A's I and Q means (`DC_ESTIM_AI`, `DC_ESTIM_AQ`); channel B's
/// follow at 6 and 7.
const CHANNEL_A_I: u8 = 4;

/// libusdr's accumulation setting for calibration (`acc_idx`, "TODO: FIXME" in its
/// source), which also divides the means.
const ACCUMULATION_SETTING: u16 = 1;

/// The sample full scale the means are taken against (`range`).
const FULL_SCALE: f64 = 32_767.0;

/// How often libusdr polls for a finished measurement before giving up.
const POLLS: u32 = 8000;

/// What one estimator poll found (`phy_rfe_pwrdc_get`).
enum Poll {
    /// No measurement finished since the one at the start (`-EAGAIN`).
    Pending,

    /// A finished measurement's I and Q means, 16.16 fixed point.
    Done(i32, i32),
}

/// Measures the power at DC after channel `channel`'s NCO (0 for A, 1 for B), in
/// thousandths of a dB of full scale, as libusdr's `phy_do_meas_nco_avg` does: set the
/// accumulation, hold and run the estimator, note its generation, then poll every
/// millisecond until it moves and the means read between two equal generations.
///
/// # Errors
///
/// [`Error::Timeout`] if no measurement finishes in 8000 polls (libusdr's `-EAGAIN`); any
/// bus failure.
pub(crate) fn measure(bus: &mut dyn Bus, channel: u8) -> Result<i32, Error> {
    write(bus, ACCUMULATION, ACCUMULATION_SETTING)?;
    write(bus, HOLD, 1)?;
    bus.sleep(Duration::from_micros(1));
    write(bus, HOLD, 0)?;
    let start = read(bus, GENERATION)?;
    for _ in 0..POLLS {
        if let Poll::Done(i, q) = poll(bus, start, channel)? {
            return Ok(millidecibels(i, q));
        }
        bus.sleep(Duration::from_millis(1));
    }
    Err(Error::Timeout)
}

/// One poll: if the generation moved from `start`, the channel's means, read again until
/// the generation holds across them.
fn poll(bus: &mut dyn Bus, start: u32, channel: u8) -> Result<Poll, Error> {
    let i_addr = CHANNEL_A_I + 2 * channel;
    loop {
        let generation = read(bus, GENERATION)?;
        if generation == start {
            return Ok(Poll::Pending);
        }
        let i = read(bus, i_addr)?;
        let q = read(bus, i_addr + 1)?;
        if read(bus, GENERATION)? == generation {
            let signed = |word: u32| i32::from_ne_bytes(word.to_ne_bytes());
            return Ok(Poll::Done(signed(i), signed(q)));
        }
    }
}

/// The power of 16.16 means `i` and `q` in thousandths of a dB of full scale, in libusdr's
/// double arithmetic, floored at -180 dB and truncated to an integer.
fn millidecibels(i: i32, q: i32) -> i32 {
    let scale =
        |mean: i32| f64::from(mean) / f64::from(ACCUMULATION_SETTING) / 65_536.0 / FULL_SCALE;
    let (i, q) = (scale(i), scale(q));
    let power = (i * i + q * q).max(1e-18);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "libusdr truncates the double to int; it lies within -180000..=+7000"
    )]
    let millidecibels = (1000.0 * 10.0 * power.log10()) as i32;
    millidecibels
}

/// Writes estimator register `addr`.
fn write(bus: &mut dyn Bus, addr: u8, value: u16) -> Result<(), Error> {
    access(
        bus,
        EstimatorAccess::new().with_addr(addr).with_value(value),
    )
}

/// Selects the word at `addr`, then reads it.
fn read(bus: &mut dyn Bus, addr: u8) -> Result<u32, Error> {
    access(bus, EstimatorAccess::new().with_read(true).with_addr(addr))?;
    let reg = Phy::Rx as u32;
    let mut word = [0];
    bus.read_regs(reg, &mut word).reading(FPGA, reg)?;
    Ok(word[0])
}

/// Sends one access word to the RX port.
fn access(bus: &mut dyn Bus, access: EstimatorAccess) -> Result<(), Error> {
    let reg = Phy::Rx as u32;
    let word = access.with_bank(u7::new(BANK)).to_raw();
    bus.write_regs(reg, &[word]).writing(FPGA, reg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_words_are_libusdrs() {
        // `_phy_rx_reg(.., true, 6, 1, 1)` and `(.., false, 6, 4, 0)`.
        let accumulate = EstimatorAccess::new()
            .with_bank(u7::new(BANK))
            .with_addr(ACCUMULATION)
            .with_value(1);
        assert_eq!(accumulate.to_raw(), 0x0601_0001);
        let select = EstimatorAccess::new()
            .with_read(true)
            .with_bank(u7::new(BANK))
            .with_addr(CHANNEL_A_I);
        assert_eq!(select.to_raw(), 0x8604_0000);
    }

    #[test]
    fn power_is_floored_at_minus_180_db_and_full_scale_is_0_db() {
        assert_eq!(millidecibels(0, 0), -180_000);
        // 32767 · 65536 is full scale on I alone.
        assert_eq!(millidecibels(32_767 * 65_536, 0), 0);
        // Half scale: -6.0206 dB, truncated towards zero.
        assert_eq!(millidecibels(32_767 * 32_768, 0), -6020);
    }
}
