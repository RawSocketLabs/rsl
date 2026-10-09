//! The PLL and divider plan for one sample-clock frequency (`si5332_set_layout`'s maths).
//!
//! The PLL multiplies the reference up to a VCO frequency in 2.375–2.625 GHz; high-speed
//! divider 0 and an output divider then bring it down to the sample clock. The feedback
//! divider is fractional: `IDPA_INTG + IDPA_RES / IDPA_DEN`, scaled by 128.
//!
//! This reproduces libusdr's arithmetic exactly, including its 32-bit wraparound, because
//! the register bytes must match its trace. Two departures from the YAML's datasheet
//! formula are libusdr's, kept: the integer term has no `- 512`, and the denominator
//! search can wrap (see [`Layout::new`]).

/// Lowest VCO frequency, in Hz.
const VCO_MIN_HZ: u32 = 2_375_000_000;
/// The VCO must stay below this, in Hz.
const VCO_MAX_HZ: u32 = 2_625_000_000;
/// Smallest high-speed divider ratio.
const HS_DIVIDER_MIN: u32 = 8;
/// Largest high-speed divider ratio.
const HS_DIVIDER_MAX: u32 = 255;
/// Output dividers are below this.
const OUTPUT_DIVIDER_LIMIT: u32 = 64;
/// References above this are prescaled by 2, in Hz.
const PRESCALE_ABOVE_HZ: u32 = 50_000_000;
/// The search for an exact fractional denominator stops here, and uses it if none is found.
const DENOMINATOR_MAX: u32 = 32_767;

/// The divider settings that make one sample-clock frequency from the reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Layout {
    /// The output frequency the plan makes, in Hz.
    pub(super) out_hz: u32,

    /// The VCO frequency, in Hz.
    pub(super) vco_hz: u32,

    /// High-speed divider 0's ratio.
    pub(super) hs_divider: u32,

    /// The output divider's ratio.
    pub(super) output_divider: u32,

    /// The reference prescaler ratio, 1 or 2.
    pub(super) prescaler: u32,

    /// The PLL's input frequency after the prescaler, in Hz.
    pub(super) pll_input_hz: u32,

    /// The feedback divider's integer term (`IDPA_INTG`).
    pub(super) integer: u32,

    /// The feedback divider's fractional numerator (`IDPA_RES`).
    pub(super) residue: u32,

    /// The feedback divider's fractional denominator (`IDPA_DEN`).
    pub(super) denominator: u32,

    /// The output times `output_divider` is within 14 Hz of the reference, so libusdr
    /// routes the outputs from the reference instead of the PLL.
    pub(super) from_reference: bool,
}

impl Layout {
    /// The VCO frequency, in Hz; the mixer LO divides it.
    pub(crate) const fn vco_hz(&self) -> u32 {
        self.vco_hz
    }

    /// The plan for `out_hz` from a `reference_hz` reference, or `None` where libusdr
    /// returns `-ERANGE` (high-speed divider 0 would go below 8, or no output divider below
    /// 64 keeps the VCO in range) and for a zero `out_hz`, which libusdr would divide by.
    ///
    /// Like libusdr, the fractional denominator is the first in 2..32767 whose product with
    /// the numerator is a multiple of the PLL input, computed in 32 bits: once the product
    /// wraps, the search can miss an exact denominator and fall back to 32767.
    pub(crate) fn new(reference_hz: u32, out_hz: u32) -> Option<Self> {
        if out_hz == 0 {
            return None;
        }
        let multiplier = VCO_MIN_HZ.wrapping_add(out_hz - 1) / out_hz;
        let mut output_divider = multiplier.div_ceil(HS_DIVIDER_MAX);
        let (hs_divider, vco_hz) = loop {
            if output_divider >= OUTPUT_DIVIDER_LIMIT {
                return None;
            }
            let hs_divider = multiplier.div_ceil(output_divider);
            let vco_hz = out_hz.wrapping_mul(hs_divider).wrapping_mul(output_divider);
            if hs_divider < HS_DIVIDER_MIN {
                return None;
            }
            if vco_hz < VCO_MAX_HZ {
                break (hs_divider, vco_hz);
            }
            output_divider += 1;
        };

        let (prescaler, pll_input_hz) = if reference_hz > PRESCALE_ABOVE_HZ {
            (2, reference_hz / 2)
        } else {
            (1, reference_hz)
        };
        let scaled_vco = 128 * u64::from(vco_hz);
        let integer = truncate(scaled_vco / u64::from(pll_input_hz));
        let fraction = truncate(scaled_vco - u64::from(integer) * u64::from(pll_input_hz));
        let denominator = (2..DENOMINATOR_MAX)
            .find(|&den: &u32| den.wrapping_mul(fraction) % pll_input_hz == 0)
            .unwrap_or(DENOMINATOR_MAX);
        let residue =
            truncate(u64::from(denominator) * u64::from(fraction) / u64::from(pll_input_hz));

        // The last match wins, as in libusdr's loop.
        let near_reference = (1..14_u32).rev().find(|&ratio| {
            #[expect(
                clippy::cast_possible_wrap,
                reason = "libusdr's `(int)` casts: the product wraps into a signed difference"
            )]
            let diff = (out_hz.wrapping_mul(ratio) as i32).wrapping_sub(reference_hz as i32);
            diff > -14 && diff < 14
        });

        Some(Self {
            out_hz,
            vco_hz,
            hs_divider,
            output_divider: near_reference.unwrap_or(output_divider),
            prescaler,
            pll_input_hz,
            integer,
            residue,
            denominator,
            from_reference: near_reference.is_some(),
        })
    }
}

/// Keeps the low 32 bits, as libusdr's assignments to `unsigned` do.
fn truncate(value: u64) -> u32 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "libusdr stores these in 32-bit unsigned variables"
    )]
    let low = value as u32;
    low
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The board's reference.
    const REFERENCE_HZ: u32 = 26_000_000;

    // Expected values: an independent Python model of `si5332_set_layout`'s C arithmetic.

    #[test]
    fn lowest_both_chain_clock_runs_the_pll_with_a_fractional_divider() {
        let layout = Layout::new(REFERENCE_HZ, 61_440_000).expect("in range");
        assert_eq!(
            layout,
            Layout {
                out_hz: 61_440_000,
                vco_hz: 2_396_160_000,
                hs_divider: 39,
                output_divider: 1,
                prescaler: 1,
                pll_input_hz: REFERENCE_HZ,
                integer: 11_796,
                residue: 12,
                denominator: 25,
                from_reference: false,
            }
        );
    }

    #[test]
    fn denominator_search_wraps_in_32_bits_as_libusdr_does() {
        // 1.002 MS/s: exact arithmetic finds denominator 1625, but libusdr's 32-bit
        // product wraps first and the search falls through to 32767.
        let layout = Layout::new(REFERENCE_HZ, 64_128_000).expect("in range");
        assert_eq!(layout.vco_hz, 2_436_864_000);
        assert_eq!(layout.integer, 11_996);
        assert_eq!((layout.residue, layout.denominator), (28_472, 32_767));
    }

    #[test]
    fn a_clock_at_the_reference_bypasses_the_pll() {
        let layout = Layout::new(REFERENCE_HZ, 26_000_000).expect("in range");
        assert!(layout.from_reference);
        assert_eq!(layout.output_divider, 1);
        assert_eq!(
            (layout.integer, layout.residue, layout.denominator),
            (11_776, 0, 2)
        );
    }

    #[test]
    fn a_clock_too_fast_for_the_smallest_divider_is_out_of_range() {
        // 2.625 GHz / 8 is the fastest clock high-speed divider 0 can make.
        assert_eq!(Layout::new(REFERENCE_HZ, 400_000_000), None);
    }
}
