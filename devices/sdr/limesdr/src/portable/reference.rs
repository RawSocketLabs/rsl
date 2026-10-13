//! The reference clock: what `limesdr_detect_refclk` makes of the frequency detector's
//! count, with libusdr's nearest-entry walk deliberately fixed.
//!
//! The detector counts reference cycles over 16 777 210 cycles of a 100 MHz clock (the C's
//! `fx3Cnt` and `fx3Clk`); libusdr turns the count back into hertz and looks for the
//! nominal reference it is. Its walk along the table is broken: the `ABS` macro evaluates
//! `clkTbl[i++]` twice and subtracts unsigned, so it skips entries and wraps. By measured
//! frequency, libusdr:
//!
//! | Measured | libusdr |
//! |---|---|
//! | below 10 MHz, or above 110 MHz | stops at the first entry, then logs `clkTbl[i - 1]` with unsigned `i` = 0 (out of bounds); `-EFAULT` |
//! | exactly 10 MHz | takes it for 30.72 MHz; `-EFAULT` |
//! | (10, 30.72) MHz | reads `clkTbl[5]` (out of bounds) |
//! | [30.72, 31.72] MHz | opens as 30.72 MHz |
//! | (31.72, 38.4] MHz | `-EFAULT` |
//! | (38.4, 40) MHz | reads `clkTbl[5]` |
//! | [40, 41] MHz | opens as 40 MHz |
//! | (41, 52) MHz | `-EFAULT` |
//! | [52, 110] MHz | reads `clkTbl[5]` |
//!
//! A real 40 MHz oscillator a few ppm slow lands in an undefined case. This port runs the
//! walk as written but with `ABS` evaluated once and signed: the nominal reference nearest
//! the measurement (the higher of two equally near), refused when more than 1 MHz away.

/// The nominal references libusdr knows, in hertz (`clkTbl`).
pub(crate) const REFERENCES_HZ: [u32; 5] =
    [10_000_000, 30_720_000, 38_400_000, 40_000_000, 52_000_000];

/// How far a measurement may be from its nominal reference: the C refuses `delta > 1e6`.
const TOLERANCE_HZ: u64 = 1_000_000;

/// The clock the detector's gate runs on, in hertz (`fx3Clk`).
const GATE_CLOCK_HZ: u64 = 100_000_000;

/// The gate's length in cycles of [`GATE_CLOCK_HZ`] (`fx3Cnt`).
const GATE_CYCLES: u64 = 16_777_210;

/// The frequency `count` reference cycles over the gate stand for, in hertz, truncated as
/// libusdr's estimate is. Kept wide: libusdr's 32-bit estimate wraps for counts no board
/// produces, which could alias a nominal reference.
pub(crate) fn measured_hz(count: u32) -> u64 {
    u64::from(count) * GATE_CLOCK_HZ / GATE_CYCLES
}

/// The nominal reference a measurement is: the nearest, the higher of two equally near (the
/// walk moves on while the distance does not grow), or `None` when it is more than 1 MHz
/// away.
pub(crate) fn nominal_hz(measured_hz: u64) -> Option<u32> {
    let distance = |hz: u32| measured_hz.abs_diff(hz.into());
    // `min_by_key` keeps the first of equal keys, so walking down keeps the higher.
    let nearest = REFERENCES_HZ
        .into_iter()
        .rev()
        .min_by_key(|&hz| distance(hz))?;
    (distance(nearest) <= TOLERANCE_HZ).then_some(nearest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ideal_40_mhz_count_measures_exactly_40_mhz() {
        // The count rsl-limesdr-sim derives for 40 MHz, 40e6 * 16 777 210 / 100e6.
        assert_eq!(measured_hz(6_710_884), 40_000_000);
    }

    #[test]
    fn a_slightly_slow_40_mhz_reference_is_recognised() {
        // libusdr reads past its table here.
        assert_eq!(nominal_hz(39_999_957), Some(40_000_000));
    }

    #[test]
    fn a_measurement_takes_the_nearest_reference_within_one_mhz() {
        for hz in REFERENCES_HZ {
            assert_eq!(nominal_hz(hz.into()), Some(hz), "{hz} Hz exactly");
        }
        assert_eq!(nominal_hz(9_000_000), Some(10_000_000), "1 MHz below");
        assert_eq!(nominal_hz(8_999_999), None, "just over 1 MHz below");
        assert_eq!(nominal_hz(53_000_000), Some(52_000_000), "1 MHz above");
        assert_eq!(nominal_hz(53_000_001), None, "just over 1 MHz above");
        assert_eq!(nominal_hz(0), None, "a detector that counted nothing");
    }

    #[test]
    fn between_close_references_the_nearer_wins_and_a_tie_goes_up() {
        // 38.4 and 40 MHz are 1.6 MHz apart, so their 1 MHz windows overlap.
        assert_eq!(nominal_hz(39_100_000), Some(38_400_000));
        assert_eq!(nominal_hz(39_300_000), Some(40_000_000));
        assert_eq!(nominal_hz(39_200_000), Some(40_000_000), "equally near");
    }

    #[test]
    fn counts_no_board_produces_do_not_wrap_onto_a_reference() {
        // libusdr's 32-bit estimate of this count wraps to just over 40 MHz.
        let wrapping =
            u32::try_from((u64::from(u32::MAX) + 1 + 40_000_000) * GATE_CYCLES / GATE_CLOCK_HZ + 1)
                .expect("the count fits 32 bits");
        assert!(measured_hz(wrapping) > u64::from(u32::MAX));
        assert_eq!(nominal_hz(measured_hz(wrapping)), None);
    }
}
