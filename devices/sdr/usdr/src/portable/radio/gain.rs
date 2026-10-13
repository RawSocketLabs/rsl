//! Gains: `usdr_rfic_set_gain` in `device/m2_lm6_1/usdr_ctrl.c`, as the
//! `/dm/sdr/0/rx/gain*` and `/dm/sdr/0/tx/gain*` controls reach it.
//!
//! The RX chain is LNA, mixer, RXVGA1, filter, RXVGA2 (two stages, A then B), ADC. Each
//! stage can be set alone, or [`RxGain::Auto`] splits one figure across the LNA, RXVGA1 and
//! RXVGA2's first stage. The TX chain is DAC, filter, TXVGA1, mixer, TXVGA2, PA; each VGA
//! is set alone. Like libusdr, every figure is clamped to its stage's range, never
//! rejected.

use super::super::lms6002d::LnaGain;
use super::super::script::Script;
use super::state::Radio;

/// An RX gain setting in dB, for one stage or the whole chain.
///
/// Values outside a stage's range are clamped to it, as libusdr does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RxGain {
    /// The whole chain, 0 to 85 dB, split by libusdr's policy (`rx/gain/auto`): the LNA
    /// takes the largest of its steps that fits, from a table of its gain against the
    /// tuned frequency; RXVGA1 takes up to 25 dB above its 5 dB floor, and RXVGA2's
    /// first stage the rest; its second stage goes to 0 dB.
    ///
    /// libusdr counts each RXVGA2 stage-A step as 6 dB, though the stage's steps are
    /// 3 dB, so above 25 dB past the LNA the chain gets 3 dB less than asked per step.
    /// This is libusdr's open-loop split, not a closed-loop AGC.
    Auto(i32),

    /// The LNA's gain mode (`rx/gain/lna`): 6 dB or more is maximum gain, 3 to 5 dB mid
    /// gain, less bypasses the LNA.
    Lna(i32),

    /// RXVGA1, the mixer's TIA, 5 to 31 dB (`rx/gain` and `rx/gain/vga`).
    Vga1(i32),

    /// RXVGA2 as one stage, 0 to 30 dB in 3 dB steps, rounded down: all on its first
    /// stage, with the second at 0 dB. libusdr's `rx/gain/pga` takes half this figure
    /// and doubles it.
    Vga2(i32),

    /// RXVGA2's first stage alone, 0 to 30 dB in 3 dB steps, rounded down
    /// (`rx/gain/vga2a`).
    Vga2a(i32),

    /// RXVGA2's second stage alone, 0 to 30 dB in 3 dB steps, rounded down
    /// (`rx/gain/vga2b`).
    Vga2b(i32),
}

impl RxGain {
    /// RXVGA1's lowest gain in dB (`GAIN_RX_VGA1_MIN`).
    pub(crate) const VGA1_MIN_DB: i32 = 5;

    /// RXVGA1's highest gain in dB (`GAIN_RX_VGA1_MAX`).
    pub(crate) const VGA1_MAX_DB: i32 = 31;

    /// RXVGA2's highest gain in dB, either stage or both, from 0 (`GAIN_RX_VGA2_MAX`).
    const VGA2_MAX_DB: i32 = 30;

    /// The automatic split's highest figure in dB, from 0 (`GAIN_RX_AUTO_MAX`).
    const AUTO_MAX_DB: i32 = 85;

    /// RXVGA2's step in dB.
    const VGA2_STEP_DB: i32 = 3;

    /// The RXVGA2 step for `db` (`_usdr_gain_clamp`, then `ngain / 3`).
    fn vga2_step(db: i32) -> u8 {
        let step = db.clamp(0, Self::VGA2_MAX_DB) / Self::VGA2_STEP_DB;
        u8::try_from(step).expect("invariant: 0..=30 dB is step 0..=10")
    }
}

/// A TX gain setting in dB, for one stage.
///
/// Values outside a stage's range are clamped to it, as libusdr does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxGain {
    /// TXVGA1, the baseband level into the TX mixer, -35 to -4 dB in 1 dB steps
    /// (`tx/gain/vga1`).
    Vga1(i32),

    /// TXVGA2, the RF level after the TX mixer, 0 to 25 dB in 1 dB steps. libusdr's
    /// `tx/gain`, `tx/gain/vga2` and `tx/gain/auto` all set this stage alone: its automatic
    /// TX gain is no split.
    Vga2(i32),
}

/// The LNA gain mode for `db` (`GAIN_RX_LNA` in `usdr_rfic_set_gain`).
fn lna_mode(db: i32) -> LnaGain {
    match db {
        6.. => LnaGain::Max,
        3..=5 => LnaGain::Mid,
        _ => LnaGain::Bypassed,
    }
}

/// libusdr's measured LNA gains (`s_lna_gains`): the frequency in MHz, then the gain from
/// bypass to mid and from mid to maximum, in tenths of a dB.
const LNA_STEPS: [(u32, i32, i32); 47] = [
    (230, 203, 51),
    (240, 200, 45),
    (250, 201, 42),
    (275, 188, 67),
    (300, 167, 101),
    (325, 172, 100),
    (350, 184, 88),
    (375, 188, 82),
    (400, 188, 79),
    (425, 187, 77),
    (450, 185, 76),
    (475, 182, 75),
    (500, 181, 74),
    (600, 177, 71),
    (700, 172, 68),
    (800, 162, 66),
    (900, 155, 64),
    (1000, 152, 61),
    (1100, 143, 60),
    (1200, 134, 59),
    (1300, 129, 58),
    (1400, 119, 58),
    (1500, 108, 58),
    (1600, 103, 56),
    (1700, 99, 55),
    (1800, 89, 55),
    (1900, 86, 54),
    (2000, 88, 53),
    (2100, 82, 53),
    (2200, 80, 51),
    (2300, 85, 51),
    (2400, 78, 52),
    (2500, 70, 51),
    (2600, 70, 51),
    (2700, 61, 52),
    (2800, 68, 52),
    (2900, 62, 50),
    (3000, 62, 50),
    (3100, 57, 50),
    (3200, 48, 49),
    (3300, 51, 48),
    (3400, 48, 48),
    (3500, 42, 48),
    (3600, 46, 47),
    (3700, 47, 46),
    (3800, 42, 45),
    (3850, 43, 45),
];

/// The LNA's two steps at `mhz`, in tenths of a dB, interpolated linearly in libusdr's
/// integer arithmetic (`_interpolate_lan_gains`), the frequency held to the table's ends.
fn lna_steps(mhz: u32) -> (i32, i32) {
    let (first, last) = (LNA_STEPS[0].0, LNA_STEPS[LNA_STEPS.len() - 1].0);
    let mhz = mhz.clamp(first, last);
    let above = LNA_STEPS[1..]
        .iter()
        .position(|&(entry_mhz, ..)| entry_mhz >= mhz)
        .expect("invariant: the held frequency is at most the last entry")
        + 1;
    let (low_mhz, low_mid, low_max) = LNA_STEPS[above - 1];
    let (high_mhz, high_mid, high_max) = LNA_STEPS[above];
    let into = i32::try_from(mhz - low_mhz).expect("invariant: entries are 100 MHz apart at most");
    let span = i32::try_from(high_mhz - low_mhz).expect("invariant: as above");
    // C's int division truncates towards zero, as Rust's does.
    let interpolate = |low: i32, high: i32| (high - low) * into / span + low;
    (
        interpolate(low_mid, high_mid),
        interpolate(low_max, high_max),
    )
}

/// How [`RxGain::Auto`] splits a figure (libusdr's `rx_lna`, `rx_vga1`, `rx_vga2a`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AutoSplit {
    /// The LNA's gain mode.
    lna: LnaGain,

    /// RXVGA1's gain above its 5 dB floor, in dB.
    vga1_db: i32,

    /// RXVGA2's first-stage step.
    vga2a: u8,
}

impl AutoSplit {
    /// The split for `db` at an RX frequency of `lo_hz`.
    fn for_gain(db: i32, lo_hz: u32) -> Self {
        let (mid, max) = lna_steps(lo_hz / 1_000_000);
        let to_mid = (mid + 5) / 10;
        let to_max = (mid + max + 5) / 10;
        let db = db.clamp(0, RxGain::AUTO_MAX_DB);
        let (lna, rest) = if db >= to_max {
            (LnaGain::Max, db - to_max)
        } else if db >= to_mid {
            (LnaGain::Mid, db - to_mid)
        } else {
            (LnaGain::Bypassed, db)
        };
        if rest <= 25 {
            return Self {
                lna,
                vga1_db: rest,
                vga2a: 0,
            };
        }
        // Whole 6 dB stage-A steps over 25 dB, rounding to nearest; RXVGA1 takes the rest.
        let steps = (rest - 25 + 5) / 6;
        Self {
            lna,
            vga1_db: rest - 6 * steps,
            vga2a: u8::try_from(steps).expect("invariant: at most (85 - 25 + 5) / 6"),
        }
    }
}

impl Radio {
    /// Sets an RX gain (`usdr_rfic_set_gain` for the RX stages). The RXVGA2 stage steps are
    /// recorded before they are written, as libusdr does, so a failed write still changes
    /// what the next DC calibration restores.
    pub(crate) fn set_rx_gain(&mut self, gain: RxGain) -> Script {
        let (lms, rx) = (&mut self.lms, &mut self.rx);
        match gain {
            RxGain::Auto(db) => {
                let split = AutoSplit::for_gain(db, rx.lo_hz);
                rx.vga1_db = u8::try_from(split.vga1_db).expect("invariant: 0..=25 dB");
                rx.vga2a = split.vga2a;
                rx.vga2b = 0;
                lms.set_lna_gain(split.lna)
                    .extend(&lms.set_rx_vga1_gain(RxGain::VGA1_MIN_DB + split.vga1_db))
                    .extend(&lms.set_rx_vga2_gain(rx.vga2a, rx.vga2b))
            }
            RxGain::Lna(db) => lms.set_lna_gain(lna_mode(db)),
            RxGain::Vga1(db) => {
                let db = db.clamp(RxGain::VGA1_MIN_DB, RxGain::VGA1_MAX_DB);
                rx.vga1_db = u8::try_from(db - RxGain::VGA1_MIN_DB).expect("invariant: 0..=26 dB");
                lms.set_rx_vga1_gain(db)
            }
            RxGain::Vga2(db) => {
                rx.vga2a = RxGain::vga2_step(db);
                rx.vga2b = 0;
                lms.set_rx_vga2_gain(rx.vga2a, rx.vga2b)
            }
            RxGain::Vga2a(db) => {
                rx.vga2a = RxGain::vga2_step(db);
                lms.set_rx_vga2_gain(rx.vga2a, rx.vga2b)
            }
            RxGain::Vga2b(db) => {
                rx.vga2b = RxGain::vga2_step(db);
                lms.set_rx_vga2_gain(rx.vga2a, rx.vga2b)
            }
        }
    }

    /// Sets a TX gain (`usdr_rfic_set_gain` for the TX stages); the LMS6002D's operations
    /// hold each figure to its stage's range.
    pub(crate) fn set_tx_gain(&mut self, gain: TxGain) -> Script {
        match gain {
            TxGain::Vga1(db) => self.lms.set_tx_vga1_gain(db),
            TxGain::Vga2(db) => self.lms.set_tx_vga2_gain(db),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lna_thresholds_sit_at_3_and_6_db() {
        assert_eq!(lna_mode(i32::MIN), LnaGain::Bypassed);
        assert_eq!(lna_mode(2), LnaGain::Bypassed);
        assert_eq!(lna_mode(3), LnaGain::Mid);
        assert_eq!(lna_mode(5), LnaGain::Mid);
        assert_eq!(lna_mode(6), LnaGain::Max);
        assert_eq!(lna_mode(i32::MAX), LnaGain::Max);
    }

    #[test]
    fn vga2_steps_are_3_db_rounded_down_and_clamped() {
        assert_eq!(RxGain::vga2_step(-1), 0);
        assert_eq!(RxGain::vga2_step(2), 0);
        assert_eq!(RxGain::vga2_step(3), 1);
        assert_eq!(RxGain::vga2_step(29), 9);
        assert_eq!(RxGain::vga2_step(30), 10);
        assert_eq!(RxGain::vga2_step(i32::MAX), 10);
    }

    #[test]
    fn lna_steps_are_the_table_at_its_entries_and_held_beyond_its_ends() {
        assert_eq!(lna_steps(1000), (152, 61));
        assert_eq!(lna_steps(0), lna_steps(230));
        assert_eq!(lna_steps(230), (203, 51));
        assert_eq!(lna_steps(u32::MAX), (43, 45));
    }

    #[test]
    fn lna_steps_interpolate_between_entries_truncating_towards_zero() {
        // Halfway from 1000 MHz (152, 61) to 1100 MHz (143, 60): -9 * 50 / 100 is -4.5,
        // truncated to -4; -1 * 50 / 100 truncates to 0.
        assert_eq!(lna_steps(1050), (148, 61));
    }

    #[test]
    fn auto_gain_spends_the_lna_first_then_vga1_then_vga2() {
        // At 1 GHz the LNA's steps are 15.2 and 6.1 dB: mid at 15 dB, maximum at 21 dB.
        let at = |db| AutoSplit::for_gain(db, 1_000_000_000);
        assert_eq!(
            at(10),
            AutoSplit {
                lna: LnaGain::Bypassed,
                vga1_db: 10,
                vga2a: 0
            }
        );
        assert_eq!(at(15).lna, LnaGain::Mid);
        assert_eq!((at(21).lna, at(21).vga1_db), (LnaGain::Max, 0));
        assert_eq!((at(46).vga1_db, at(46).vga2a), (25, 0));
        // 47 dB leaves 26 over the LNA: one 6 dB stage-A step, rounded to nearest.
        assert_eq!((at(47).vga1_db, at(47).vga2a), (20, 1));
    }

    #[test]
    fn auto_gain_clamps_to_0_and_85_db() {
        let at = |db| AutoSplit::for_gain(db, 1_000_000_000);
        assert_eq!(at(-5), at(0));
        assert_eq!(at(1000), at(85));
        let top = at(85);
        assert_eq!(top.lna, LnaGain::Max);
        assert_eq!(top.vga1_db + 6 * i32::from(top.vga2a), 85 - 21);
    }
}
