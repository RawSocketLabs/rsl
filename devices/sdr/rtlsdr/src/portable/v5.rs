//! The `NESDR SMArt v5`'s sample rate, tuning and gain, as the sweep captures fit them.
//!
//! Every value here is fitted to librtlsdr's traffic over the owner's V5 (GPLOBS-V5-0005
//! to -0060, `RTL-OBS-11` to `RTL-OBS-18`): the plans reproduce, register for register, the
//! state `rtl_sdr` left before streaming in each of those 700-odd sessions. They are pure:
//! [`crate::Device`] sends them.

use super::r820t2::Registers;
use super::r820t2::pll::PllDivider;

/// The crystal both chips run from, in hertz: the resampler ratio and the PLL both fit it
/// exactly. prov: RTL-OBS-11
pub const XTAL_HZ: u32 = 28_800_000;

/// The sample rates evidenced, in S/s: each range's ends were tried and accepted, and the
/// rates tried between them refused. prov: RTL-OBS-12
pub const RATE_RANGES: [(u32, u32); 2] = [(225_001, 300_000), (900_001, 3_200_000)];

/// Fractional bits of the resampler ratio. prov: RTL-OBS-12
const RATIO_FRACTION_BITS: u32 = 22;

/// The ratio's bits the dongle is sent: bits 27 to 2, so the rates under 450 kS/s lose
/// their bit 28. prov: RTL-OBS-12
const RATIO_MASK: u32 = 0x0fff_fffc;

/// The paged-block page 1 registers of the ratio's high and low halves. prov: RTL-OBS-12
pub const RATIO_REGISTERS: [u8; 2] = [0x9f, 0xa1];

/// Page 1 register of the IF word's first byte; the next two follow. prov: RTL-OBS-13
pub const IF_REGISTER: u8 = 0x19;

/// Bits of the IF word. prov: RTL-OBS-13
const IF_BITS: u32 = 22;

/// Page 1 registers written `0x00` after the ratio. prov: RTL-OBS-14
pub const AFTER_RATIO: [u8; 2] = [0x3f, 0x3e];

/// Bytes written, in order, to the gate register after those. prov: RTL-OBS-14
pub const AFTER_RATIO_GATE: [u8; 2] = [0x14, 0x10];

/// The tuner's R10, the first channel-filter register; R11 follows. prov: RTL-OBS-13
pub const FILTER_REGISTER: u8 = 0x0a;

/// The tuner's R23, the first band register; R26 and R27 are its others. prov: RTL-OBS-17
pub const BAND_REGISTERS: [u8; 3] = [0x17, 0x1a, 0x1b];

/// The tuner's R16, the first PLL register. prov: RTL-OBS-15
pub const PLL_REGISTER: u8 = 0x10;

/// The tuner's R18, the PLL retry register. prov: RTL-OBS-16
pub const RETRY_REGISTER: u8 = 0x12;

/// Registers read, from R0, before the PLL write and after it. prov: RTL-OBS-16
pub const PLL_READS: [usize; 2] = [5, 3];

/// The tuner's gain registers: R5, R7, R12. prov: RTL-OBS-18
pub const GAIN_REGISTERS: [u8; 3] = [0x05, 0x07, 0x0c];

/// One row of the rate table: rates up to `max_rate` S/s use this IF and channel filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RateRow {
    /// The row's highest rate.
    max_rate: u32,
    /// The IF, in hertz: the LO sits this far above the tuned frequency.
    if_hz: u32,
    /// Tuner R10.
    r10: u8,
    /// Tuner R11.
    r11: u8,
}

/// The rate table, by ascending `max_rate`. prov: RTL-OBS-13
const RATE_TABLE: [RateRow; 16] = [
    RateRow {
        max_rate: 300_000,
        if_hz: 2_125_000,
        r10: 0xc6,
        r11: 0xe6,
    },
    RateRow {
        max_rate: 1_200_000,
        if_hz: 1_700_000,
        r10: 0xc6,
        r11: 0xeb,
    },
    RateRow {
        max_rate: 1_450_000,
        if_hz: 1_575_000,
        r10: 0xc6,
        r11: 0xec,
    },
    RateRow {
        max_rate: 1_550_000,
        if_hz: 1_525_000,
        r10: 0xc6,
        r11: 0xed,
    },
    RateRow {
        max_rate: 1_600_000,
        if_hz: 1_500_000,
        r10: 0xc6,
        r11: 0xee,
    },
    RateRow {
        max_rate: 1_700_000,
        if_hz: 1_450_000,
        r10: 0xc6,
        r11: 0xef,
    },
    RateRow {
        max_rate: 1_800_000,
        if_hz: 1_750_000,
        r10: 0xc6,
        r11: 0xac,
    },
    RateRow {
        max_rate: 1_900_000,
        if_hz: 1_700_000,
        r10: 0xc6,
        r11: 0xad,
    },
    RateRow {
        max_rate: 1_950_000,
        if_hz: 1_675_000,
        r10: 0xc6,
        r11: 0xae,
    },
    RateRow {
        max_rate: 2_050_000,
        if_hz: 1_625_000,
        r10: 0xc6,
        r11: 0xaf,
    },
    RateRow {
        max_rate: 2_080_000,
        if_hz: 1_640_000,
        r10: 0xc6,
        r11: 0xcf,
    },
    RateRow {
        max_rate: 2_180_000,
        if_hz: 1_940_000,
        r10: 0xc6,
        r11: 0x8c,
    },
    RateRow {
        max_rate: 2_280_000,
        if_hz: 1_890_000,
        r10: 0xc6,
        r11: 0x8d,
    },
    RateRow {
        max_rate: 2_330_000,
        if_hz: 1_865_000,
        r10: 0xc6,
        r11: 0x8e,
    },
    RateRow {
        max_rate: 2_430_000,
        if_hz: 1_815_000,
        r10: 0xc6,
        r11: 0x8f,
    },
    RateRow {
        max_rate: 3_200_000,
        if_hz: 3_570_000,
        r10: 0xd6,
        r11: 0x6b,
    },
];

/// What a sample rate programs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RatePlan {
    /// The resampler ratio, as sent.
    pub ratio: u32,
    /// The IF, in hertz.
    pub if_hz: u32,
    /// The three IF bytes from page 1 [`IF_REGISTER`] on.
    pub if_bytes: [u8; 3],
    /// Tuner R10 and R11.
    pub filter: [u8; 2],
}

impl RatePlan {
    /// The plan for `rate` S/s, if it is in [`RATE_RANGES`].
    #[must_use]
    pub fn new(rate: u32) -> Option<Self> {
        if !RATE_RANGES
            .iter()
            .any(|&(lo, hi)| (lo..=hi).contains(&rate))
        {
            return None;
        }
        let row = RATE_TABLE.iter().find(|row| rate <= row.max_rate)?;
        let ratio = u32::try_from((u64::from(XTAL_HZ) << RATIO_FRACTION_BITS) / u64::from(rate))
            .ok()?
            & RATIO_MASK;
        let word = (u64::from(row.if_hz) << IF_BITS) / u64::from(XTAL_HZ);
        let word = (1u64 << IF_BITS).wrapping_sub(word) & ((1 << IF_BITS) - 1);
        let [_, _, _, _, _, high, mid, low] = word.to_be_bytes();
        Some(Self {
            ratio,
            if_hz: row.if_hz,
            if_bytes: [high, mid, low],
            filter: [row.r10, row.r11],
        })
    }
}

/// The lowest VCO frequency the divider choice allows, in kHz. prov: RTL-OBS-15
const VCO_MIN_KHZ: u64 = 1_770_000;

/// The LO's kHz, rounded, must stay below this: no PLL write at or above it.
/// prov: RTL-OBS-15
const LO_MAX_KHZ: u64 = 1_770_000;

/// The mixer dividers, smallest first; `SEL_DIV`'s code is the index. prov: RTL-OBS-15
const MIXER_DIVIDERS: [u64; 6] = [2, 4, 8, 16, 32, 64];

/// R16's bits under `SEL_DIV` ([4:0]) in every tune. prov: RTL-OBS-15
const R16_LOW: u8 = 0x04;

/// R17 to R19 in the PLL write. prov: RTL-OBS-15
const R17_TO_R19: [u8; 3] = [0xbb, 0x80, 0x31];

/// R18 written when the PLL has not locked, before reading again. prov: RTL-OBS-16
pub const R18_RETRY: u8 = 0x60;

/// R2's lock bit. prov: RTL-OBS-16
pub const LOCK_BIT: u8 = 0x40;

/// R26's PLL auto-tune clock bits ([3:2]): clear during the PLL write, `0b10` after it.
/// prov: RTL-OBS-16
const R26_CLOCK_AFTER: u8 = 0x08;

/// R23 by LO: `(from_hz, value)`, ascending; the last row reached applies. prov: RTL-OBS-17
const R23_BANDS: [(u32, u8); 2] = [(0, 0x38), (75_000_000, 0x30)];

/// R26 (auto-tune clock bits clear) by LO. prov: RTL-OBS-17
const R26_BANDS: [(u32, u8); 3] = [(0, 0x22), (310_000_000, 0x61), (588_000_000, 0x60)];

/// R27 by LO. prov: RTL-OBS-17
const R27_BANDS: [(u32, u8); 13] = [
    (0, 0xdf),
    (50_000_000, 0xbe),
    (55_000_000, 0x8b),
    (60_000_000, 0x7b),
    (65_000_000, 0x69),
    (70_000_000, 0x58),
    (75_000_000, 0x44),
    (90_000_000, 0x34),
    (110_000_000, 0x24),
    (140_000_000, 0x14),
    (180_000_000, 0x13),
    (250_000_000, 0x11),
    (280_000_000, 0x00),
];

/// The value `table` gives `lo_hz`.
fn band(table: &[(u32, u8)], lo_hz: u32) -> u8 {
    table
        .iter()
        .rev()
        .find(|&&(from, _)| lo_hz >= from)
        .map_or(table[0].1, |&(_, value)| value)
}

/// What tuning to a frequency programs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TunePlan {
    /// R23, R26 (auto-tune clock bits clear) and R27, written before the PLL.
    pub bands: [u8; 3],
    /// R16 to R22.
    pub pll: [u8; 7],
    /// R26 after the PLL.
    pub r26_after: u8,
}

impl TunePlan {
    /// The plan for `frequency_hz` with an IF of `if_hz`, if the LO is in range: its kHz,
    /// rounded, times some divider reaching [`VCO_MIN_KHZ`] and itself below [`LO_MAX_KHZ`].
    #[must_use]
    pub fn new(frequency_hz: u32, if_hz: u32) -> Option<Self> {
        let lo = frequency_hz.checked_add(if_hz)?;
        let lo_khz = (u64::from(lo) + 500) / 1000;
        if lo_khz >= LO_MAX_KHZ {
            return None;
        }
        let (code, divider) = MIXER_DIVIDERS
            .iter()
            .enumerate()
            .find(|&(_, &d)| lo_khz * d >= VCO_MIN_KHZ)?;
        // N = Nint + Nfra = VCO / (2·fref), in units of 2^-16, rounded to nearest; Ndiv = 2N.
        let reference = u64::from(XTAL_HZ);
        let n_q16 = (((u64::from(lo) * divider) << 16) + reference) / (2 * reference);
        let divider = PllDivider::from_ndiv_q16(u32::try_from(2 * n_q16).ok()?)?;
        let mut registers = Registers::default();
        divider.write(&mut registers);
        let r16 = (u8::try_from(code).ok()? << 5) | R16_LOW;
        let [r17, r18, r19] = R17_TO_R19;
        let pll = [
            r16,
            r17,
            r18,
            r19,
            registers.byte(0x14)?,
            registers.byte(0x15)?,
            registers.byte(0x16)?,
        ];
        let r26 = band(&R26_BANDS, lo);
        Some(Self {
            bands: [band(&R23_BANDS, lo), r26, band(&R27_BANDS, lo)],
            pll,
            r26_after: r26 | R26_CLOCK_AFTER,
        })
    }
}

/// librtlsdr's labels for the manual gain steps, in tenths of a dB, as `rtl_test` prints
/// them; labels only, not measurements (R820-GAP-04). prov: RTL-OBS-18
pub const GAIN_LABELS_TENTH_DB: [u16; 29] = [
    0, 9, 14, 27, 37, 77, 87, 125, 144, 157, 166, 197, 207, 229, 254, 280, 297, 328, 338, 364, 372,
    386, 402, 421, 434, 439, 445, 480, 496,
];

/// Each manual step's LNA and mixer codes: the LNA leads by one on odd steps, and the last
/// step raises both. prov: RTL-OBS-18
const GAIN_CODES: [(u8, u8); 29] = [
    (0, 0),
    (1, 0),
    (1, 1),
    (2, 1),
    (2, 2),
    (3, 2),
    (3, 3),
    (4, 3),
    (4, 4),
    (5, 4),
    (5, 5),
    (6, 5),
    (6, 6),
    (7, 6),
    (7, 7),
    (8, 7),
    (8, 8),
    (9, 8),
    (9, 9),
    (10, 9),
    (10, 10),
    (11, 10),
    (11, 11),
    (12, 11),
    (12, 12),
    (13, 12),
    (13, 13),
    (14, 13),
    (15, 14),
];

/// R5 in manual gain: LNA manual, its code in [3:0]. prov: RTL-OBS-18
const R5_MANUAL: u8 = 0x10;

/// R7 in manual gain: mixer manual, its code in [3:0]. prov: RTL-OBS-18
const R7_MANUAL: u8 = 0x60;

/// R12 in manual gain. prov: RTL-OBS-18
const R12_MANUAL: u8 = 0x68;

/// R5, R7 and R12 in automatic gain: R5 and R7 as the open leaves them. prov: RTL-OBS-18
const AUTO: [u8; 3] = [0x03, 0x75, 0x6b];

/// A gain setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Gain {
    /// The tuner's own gain control.
    Auto,
    /// A manual step.
    Manual(GainStep),
}

/// A manual gain step, 0 to 28 ([`GAIN_CODES`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GainStep(u8);

impl GainStep {
    /// Step `step`, if there is one.
    #[must_use]
    pub const fn new(step: u8) -> Option<Self> {
        if (step as usize) < GAIN_LABELS_TENTH_DB.len() {
            Some(Self(step))
        } else {
            None
        }
    }

    /// librtlsdr's label for the step.
    #[must_use]
    pub const fn label_tenth_db(self) -> u16 {
        GAIN_LABELS_TENTH_DB[self.0 as usize]
    }
}

impl Gain {
    /// R5, R7 and R12.
    #[must_use]
    pub const fn registers(self) -> [u8; 3] {
        match self {
            Self::Auto => AUTO,
            Self::Manual(GainStep(k)) => {
                let (lna, mixer) = GAIN_CODES[k as usize];
                [R5_MANUAL | lna, R7_MANUAL | mixer, R12_MANUAL]
            }
        }
    }
}
