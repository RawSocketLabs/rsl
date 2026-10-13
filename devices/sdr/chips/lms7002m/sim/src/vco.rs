//! The VCO tuning comparators: what `CGEN_0x008C` and `SXX_0x0123` report about the
//! capacitor bank code a driver has just written.
//!
//! libusdr tunes each synthesizer by writing a cap code (`CSW_VCO`) and reading two
//! comparator bits (`VCO_CMPHO`, `VCO_CMPLO`; `hw/lms7002m/lms7002m.c`:
//! `lms7002m_cgen_trim_vco`, `lms7002m_sxx_trim_vco`), which it reads as `(CMPHO << 1) |
//! CMPLO`: 0 low, 1 fail, 2 OK, 3 high (`enum lms7002m_vco_comparator`). Its search
//! (`_lms7002m_vco_range`) assumes that, as the code rises, the reading goes from low
//! through OK to high: the binary search moves up on low and down on OK or high, and the
//! linear scan closes a range at the first reading after it that is not OK.
//!
//! The model, a sim choice with no silicon evidence beyond that ordering:
//! - Each VCO's free-running frequency falls linearly with the code, from its span's top
//!   at code 0 to its bottom at code 255. The SXX spans are libusdr's own tuning limits
//!   (`enum sxx_vco_params`), not datasheet figures. The CGEN span is a sim choice, 1.9 to
//!   2.9 GHz: wider than libusdr's nominal 2.0 to 2.7 GHz (`enum cgen_vco_params`), which
//!   `lms7002m_cgen_tune` itself leaves with only a warning ("VCO may not lock") for
//!   common rates (30.72 MS/s programs 1.966 GHz), so those can lock.
//! - The PLL's target is the reference times the feedback divider the registers program:
//!   `INT_SDM + 1` (CGEN) or `INT_SDM + 4` (SXX, as `lms7002m_sxx_tune` writes `nint - 4`)
//!   plus `FRAC_SDM / 2^20`.
//! - A code whose frequency is within [`WINDOW`] codes' worth of the target reads OK; a
//!   faster VCO reads low, a slower one high. "Fail" is never reported.
//! - The comparators settle at once: libusdr's slow variants, which sleep before or after
//!   the read, read the same.
//! - Only the comparator bits are modelled; the register's other bits read as written
//!   (zero, since nothing writes them).

/// `CGEN_0x0087`: the fractional divider's low 16 bits.
const CGEN_FRAC_LOW: u16 = 0x0087;
/// `CGEN_0x0088`: `INT_SDM` (bits 13:4) and the fraction's top 4 bits (3:0).
const CGEN_INT_FRAC: u16 = 0x0088;
/// `CGEN_0x008B`: `CSW_VCO` in bits 8:1.
const CGEN_VCO_CAP: u16 = 0x008b;
/// `CGEN_0x008C`: the comparators.
pub(crate) const CGEN_COMPARATORS: u16 = 0x008c;

/// `SXX_0x011D`: the fractional divider's low 16 bits.
const SXX_FRAC_LOW: u16 = 0x011d;
/// `SXX_0x011E`: `INT_SDM` (bits 13:4) and the fraction's top 4 bits (3:0).
const SXX_INT_FRAC: u16 = 0x011e;
/// `SXX_0x0121`: `CSW_VCO` in bits 10:3, `SEL_VCO` in bits 2:1.
const SXX_VCO_CAP: u16 = 0x0121;
/// `SXX_0x0123`: the comparators.
pub(crate) const SXX_COMPARATORS: u16 = 0x0123;

/// `VCO_CMPHO` (bit 13) and `VCO_CMPLO` (bit 12) in both comparator registers.
const COMPARATOR_BITS: u16 = 0b11 << 12;
/// Both comparators set: the VCO is too slow for the target (`LMS7002M_VCO_HIGH`).
const HIGH: u16 = 0b11 << 12;
/// Only `VCO_CMPHO` set: in range (`LMS7002M_VCO_OK`).
const OK: u16 = 0b10 << 12;
/// Neither set: the VCO is too fast (`LMS7002M_VCO_LOW`).
const LOW: u16 = 0;
/// Only `VCO_CMPLO` set: libusdr's `LMS7002M_VCO_FAIL`, never produced by the model; a
/// board can force it at one cap code ([`crate::Lms7002m::with_comparator_failure`]).
pub(crate) const FAIL: u16 = 0b01 << 12;

/// Half the width, in cap codes, of the band that reads OK. A sim choice: wide enough for
/// libusdr's search to find a range of several codes.
const WINDOW: i64 = 4;
/// The highest cap code.
const CAP_MAX: i64 = 255;

/// The CGEN VCO's span in hertz: a sim choice around libusdr's `CGEN_VCO_MIN` (2 GHz)
/// and `CGEN_VCO_MAX` (2.7 GHz); see the module docs.
const CGEN_SPAN: (u64, u64) = (1_900_000_000, 2_900_000_000);
/// The SXX VCOs' spans in hertz by `SEL_VCO`: libusdr's `SXX_VCOL_*`, `SXX_VCOM_*` and
/// `SXX_VCOH_*`. Code 3 selects nothing the source names; the model gives it no span.
const SXX_SPANS: [(u64, u64); 3] = [
    (1_900_000_000, 2_611_000_000),
    (2_481_000_000, 3_377_000_000),
    (3_153_000_000, 3_999_000_000),
];

/// The comparator bits `CGEN_0x008C` reports, given a register reader and the reference.
pub(crate) fn cgen(reg: impl Fn(u16) -> u16, reference: u64) -> u16 {
    let target = target(reg(CGEN_INT_FRAC), reg(CGEN_FRAC_LOW), 1, reference);
    let cap = cgen_cap(&reg);
    compare(CGEN_SPAN, cap, target)
}

/// The comparator bits `SXX_0x0123` reports.
pub(crate) fn sxx(reg: impl Fn(u16) -> u16, reference: u64) -> u16 {
    let target = target(reg(SXX_INT_FRAC), reg(SXX_FRAC_LOW), 4, reference);
    let vco_cap = reg(SXX_VCO_CAP);
    let cap = sxx_cap(&reg);
    let Some(&span) = SXX_SPANS.get(usize::from((vco_cap >> 1) & 0b11)) else {
        return LOW;
    };
    compare(span, cap, target)
}

/// The CGEN cap code last written (`CSW_VCO_CGEN`).
pub(crate) fn cgen_cap(reg: impl Fn(u16) -> u16) -> u16 {
    (reg(CGEN_VCO_CAP) >> 1) & 0xff
}

/// The SXX cap code last written (`CSW_VCO`).
pub(crate) fn sxx_cap(reg: impl Fn(u16) -> u16) -> u16 {
    (reg(SXX_VCO_CAP) >> 3) & 0xff
}

/// The comparator bits of the register at `addr` with its other bits from `stored`.
pub(crate) fn merge(stored: u16, comparators: u16) -> u16 {
    stored & !COMPARATOR_BITS | comparators
}

/// The PLL's target in hertz, times 2^20: `reference * (INT_SDM + offset + FRAC / 2^20)`.
fn target(int_frac: u16, frac_low: u16, offset: u64, reference: u64) -> u64 {
    let int = u64::from((int_frac >> 4) & 0x3ff) + offset;
    let frac = (u64::from(int_frac & 0xf) << 16) | u64::from(frac_low);
    reference * ((int << 20) + frac)
}

/// The reading for cap code `cap` of a VCO spanning `(bottom, top)` hertz against a
/// target given in hertz times 2^20.
fn compare((bottom, top): (u64, u64), cap: u16, target: u64) -> u16 {
    // Everything times 255 * 2^20, so the line stays in integers.
    let scale = |hz: u64| (i128::from(hz) * i128::from(CAP_MAX)) << 20;
    let step = i128::from(top - bottom) << 20;
    let free_running = scale(top) - i128::from(cap) * step;
    let offset = free_running - i128::from(target) * i128::from(CAP_MAX);
    let window = i128::from(WINDOW) * step;
    if offset > window {
        LOW
    } else if offset < -window {
        HIGH
    } else {
        OK
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 26 MHz reference, as on the XSDR.
    const REFERENCE: u64 = 26_000_000;

    /// The reading for `cap` when the CGEN PLL targets `n` times the reference.
    fn cgen_at(n: u16, cap: u16) -> u16 {
        cgen(
            |addr| match addr {
                CGEN_INT_FRAC => (n - 1) << 4,
                CGEN_VCO_CAP => cap << 1,
                _ => 0,
            },
            REFERENCE,
        )
    }

    #[test]
    fn the_reading_runs_low_then_ok_then_high_as_libusdrs_search_assumes() {
        // 90 * 26 MHz = 2.34 GHz, inside the CGEN span.
        let readings: Vec<u16> = (0..=255).map(|cap| cgen_at(90, cap)).collect();
        let first_ok = readings
            .iter()
            .position(|&r| r == OK)
            .expect("a code locks");
        let first_high = readings.iter().position(|&r| r == HIGH).expect("then high");
        assert!(readings[..first_ok].iter().all(|&r| r == LOW));
        assert!(readings[first_ok..first_high].iter().all(|&r| r == OK));
        assert!(readings[first_high..].iter().all(|&r| r == HIGH));
        let width = i64::try_from(first_high - first_ok).expect("a short band");
        assert!(
            (2 * WINDOW..=2 * WINDOW + 1).contains(&width),
            "the codes within WINDOW of the target read OK"
        );
    }

    #[test]
    fn a_target_outside_the_span_never_reads_ok() {
        // 120 * 26 MHz = 3.12 GHz, above the CGEN span's 2.9 GHz.
        assert!((0..=255).all(|cap| cgen_at(120, cap) == HIGH));
        // 70 * 26 MHz = 1.82 GHz, below its 1.9 GHz.
        assert!((0..=255).all(|cap| cgen_at(70, cap) == LOW));
    }

    #[test]
    fn the_sxx_reading_follows_the_selected_vco() {
        // 3.6 GHz: only VCOH (SEL_VCO 2) reaches it.
        let at = |sel: u16, cap: u16| {
            sxx(
                |addr| match addr {
                    SXX_INT_FRAC => (138 - 4) << 4,
                    SXX_VCO_CAP => (cap << 3) | (sel << 1),
                    _ => 0,
                },
                REFERENCE,
            )
        };
        assert!((0..=255).any(|cap| at(2, cap) == OK));
        assert!((0..=255).all(|cap| at(0, cap) == HIGH), "VCOL is too slow");
        assert!((0..=255).all(|cap| at(3, cap) == LOW), "code 3 has no VCO");
    }
}
