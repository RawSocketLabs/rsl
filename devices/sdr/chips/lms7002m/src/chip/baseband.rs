//! The receive and transmit basebands: filter paths and their codes, the receive PGA and
//! the transmit amplifier's gain (`lms7002m_rbb_*`, `lms7002m_tbb_*`).

use core::num::NonZeroU32;

use bnb::{u2, u3, u5, u6, u11};

use super::Lms7002m;
use crate::batch::Batch;
use crate::reg::Reg;
use crate::register::Register;
use crate::regs::{
    PgaInput, RbbControl, RbbHighFilter, RbbInput, RbbLowFilter, RbbPga, RbbPgaFeedback,
    TbbAmplifier, TbbControl, TbbFilterCal, TbbS5, TbbTest,
};

/// The TX amplifier gain code `lms7002m_create` caches (`reg_tbb_gc`).
pub(super) const TBB_GAIN_CREATED: u8 = 12;

/// The receive baseband's filter: libusdr's `lms7002m_rbb_path_t`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RbbPath {
    /// The low-band filter (`RBB_LBF`).
    LowBand,

    /// The high-band filter (`RBB_HBF`).
    HighBand,

    /// No filter (`RBB_BYP`).
    Bypass,

    /// The power detector (`RBB_PDET`); libusdr writes it like the bypass.
    PowerDetector,
}

/// How the receive baseband runs: libusdr's `lms7002m_rbb_mode_t`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RbbMode {
    /// Off (`RBB_MODE_DISABLE`).
    Disable,

    /// On (`RBB_MODE_NORMAL`).
    Normal,

    /// On, fed from the TX baseband (`RBB_MODE_LOOPBACK`).
    Loopback,

    /// On; libusdr names it an ADC loopback (`RBB_MODE_LOOPBACK_ADC`) but writes it as
    /// [`Self::Normal`].
    LoopbackAdc,
}

/// The transmit baseband's filter: libusdr's `lms7002m_tbb_path_t`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TbbPath {
    /// No filter (`TBB_BYP`).
    Bypass,

    /// The ladder (low-band) filter (`TBB_LAD`).
    Ladder,

    /// The high-band filter (`TBB_HBF`).
    HighBand,
}

/// How the transmit baseband runs: libusdr's `lms7002m_tbb_mode_t`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TbbMode {
    /// Off (`TBB_MODE_DISABLE`); libusdr writes the `LB_TBB_OUT` loopback code with it.
    Disable,

    /// On (`TBB_MODE_NORMAL`).
    Normal,

    /// On, its output looped back (`TBB_MODE_LOOPBACK`).
    Loopback,

    /// As [`Self::Loopback`], I and Q swapped (`TBB_MODE_LOOPBACK_SWAPIQ`).
    LoopbackSwapIq,

    /// The DAC looped back, the filter bypassed (`TBB_MODE_LOOPBACK_DAC`).
    LoopbackDac,
}

/// A filter's codes: libusdr's `lms7002m_lpf_params_t`, as its `_def` functions compute
/// them for a bandwidth (before any calibration).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LpfParams {
    /// The RC-network code.
    pub rcc: u32,

    /// The resistor code.
    pub r: u32,

    /// The capacitor code (receive filters only).
    pub c: u32,
}

impl LpfParams {
    /// The receive filter's codes for `bandwidth` hertz (`lms7002m_rbb_lpf_def`), for the
    /// low-band filter or the high-band one. libusdr divides by the bandwidth, so it is
    /// non-zero here.
    #[must_use]
    pub fn rbb_default(bandwidth: NonZeroU32, low_band: bool) -> Self {
        let bandwidth = bandwidth.get();
        if low_band {
            // `(int)(2160000000U / bw - 103)`: unsigned, so a quotient under 103 wraps
            // negative and clamps to 0.
            let c = i32::from_le_bytes((2_160_000_000 / bandwidth).wrapping_sub(103).to_le_bytes());
            let rcc = match bandwidth {
                15_000_001.. => 5,
                10_000_001.. => 4,
                5_000_001.. => 3,
                3_000_001.. => 2,
                1_400_001.. => 1,
                _ => 0,
            };
            Self {
                rcc,
                r: 16,
                c: non_negative(c.clamp(0, 2047)),
            }
        } else {
            let hz = f64::from(bandwidth);
            #[expect(
                clippy::cast_possible_truncation,
                reason = "libusdr's (int) cast of a value the clamp then bounds"
            )]
            let (c, rcc) = ((6000.0e6 / hz - 50.0) as i32, (hz / 10e6 - 3.0) as i32);
            Self {
                rcc: non_negative(rcc.clamp(0, 7)),
                r: 16,
                c: non_negative(c.clamp(0, 255)),
            }
        }
    }

    /// The transmit filter's codes for `bandwidth` hertz (`lms7002m_tbb_lpf_def`): the
    /// ladder filter's between 1 and 20 MHz, the high-band filter's between 20 and 80 MHz,
    /// each from libusdr's fitted polynomial.
    #[must_use]
    pub fn tbb_default(bandwidth: u32, ladder: bool) -> Self {
        let (bandwidth, coefficients, rcc_step) = if ladder {
            (
                bandwidth.clamp(1_000_000, 20_000_000),
                [
                    1.298_589_036_479_58E-16,
                    -0.000_110_746_929_967_704,
                    0.002_775_934_859_910_29,
                    21.038_429_316_960_7,
                    -48.409_260_623_829_7,
                ],
                2_000_000,
            )
        } else {
            (
                bandwidth.clamp(20_000_000, 80_000_000),
                [
                    1.103_832_616_111_12E-06,
                    -0.000_210_800_032_517_545,
                    0.019_049_487_480_330_9,
                    1.433_174_459_235_28,
                    -47.695_077_929_833_3,
                ],
                1_000_000,
            )
        };
        let f = f64::from(bandwidth) / 1e6;
        let [p1, p2, p3, p4, p5] = coefficients;
        // libusdr's left-to-right evaluation, kept for bit-exact codes.
        #[expect(
            clippy::cast_possible_truncation,
            reason = "libusdr's (int) cast of a value the clamp then bounds"
        )]
        let r = (f * f * f * f * p1 + f * f * f * p2 + f * f * p3 + f * p4 + p5) as i32;
        Self {
            rcc: ((bandwidth + rcc_step / 2) / rcc_step).clamp(1, 63),
            // libusdr stores the code in an unsigned field before clamping it, so a negative
            // fit (bandwidths at the bottom of a range) clamps to 255, not 0.
            r: unsigned(r).min(255),
            c: 0,
        }
    }
}

/// A code the caller has clamped to be non-negative.
fn non_negative(code: i32) -> u32 {
    u32::try_from(code).expect("invariant: clamped to a non-negative range")
}

/// `code` as C converts an `int` to `unsigned`: its two's-complement bits.
const fn unsigned(code: i32) -> u32 {
    u32::from_le_bytes(code.to_le_bytes())
}

/// The low byte of a code: the C macros mask each field.
const fn byte(code: u32) -> u8 {
    code.to_le_bytes()[0]
}

/// The PGA's feedback RC code for each gain in dB (`lms7002m_rbb_pga`'s `rcc_corr`).
const PGA_RCC: [u8; 32] = [
    31, 30, 29, 29, 28, 27, 26, 26, 25, 24, 24, 23, 23, 22, 22, 21, 21, 20, 20, 19, 19, 19, 18, 18,
    18, 17, 17, 17, 16, 16, 16, 16,
];

impl Lms7002m {
    /// Selects the receive baseband's filter and mode (`lms7002m_rbb_path`): the
    /// selected channels' cached `EN_DIR_RBB` follows the mode (not written here), and the
    /// control and PGA-input words reach the selected banks.
    pub fn rbb_path(&mut self, path: RbbPath, mode: RbbMode) -> Batch {
        let enabled = mode != RbbMode::Disable;
        let selected = self.banks();
        for (en_dir, _) in self.en_dir.iter_mut().zip(selected).filter(|(_, on)| *on) {
            en_dir.set_rbb(enabled);
        }
        let loopback = mode == RbbMode::Loopback;
        let input = match path {
            RbbPath::LowBand => PgaInput::LowFilter,
            RbbPath::HighBand => PgaInput::HighFilter,
            RbbPath::Bypass if loopback => PgaInput::Tbb,
            RbbPath::Bypass | RbbPath::PowerDetector => PgaInput::Bypass,
        };
        let mut batch = Batch::new(self.target);
        batch.push(
            RbbControl::new()
                .with_loopback_high(loopback && path == RbbPath::HighBand)
                .with_loopback_low(loopback && path == RbbPath::LowBand)
                .with_high_filter_powered_down(path != RbbPath::HighBand)
                .with_low_filter_powered_down(path != RbbPath::LowBand)
                .with_enabled(enabled)
                .entry(),
        );
        batch.push(
            RbbInput::new()
                .with_pga_input(input)
                .with_filter_in_bias(u5::new(24))
                .with_filter_out_bias(u5::new(24))
                .entry(),
        );
        batch
    }

    /// Sets the receive PGA's gain to `gain_x10` tenths of a dB, clamped to 0–31 dB and
    /// truncated to whole dB, with its feedback codes (`lms7002m_rbb_pga`).
    pub fn rbb_pga(&self, gain_x10: i32) -> Batch {
        let db = byte(non_negative(gain_x10.clamp(0, 310) / 10));
        let capacitor = match db {
            ..8 => 3,
            8..13 => 2,
            13..21 => 1,
            _ => 0,
        };
        let mut batch = Batch::new(self.target);
        batch.push(
            RbbPga::new()
                .with_out_bias(u5::new(20))
                .with_in_bias(u5::new(20))
                .with_gain(u5::from_raw(db))
                .entry(),
        );
        batch.push(
            RbbPgaFeedback::new()
                .with_rcc(u5::from_raw(PGA_RCC[usize::from(db)]))
                .with_capacitor(capacitor)
                .entry(),
        );
        batch
    }

    /// Writes the receive filters' codes (`lms7002m_rbb_lpf_raw`): both filters get the
    /// same RC and capacitor codes, masked to each field.
    pub fn rbb_lpf(&self, params: LpfParams) -> Batch {
        let [c0, c1, ..] = params.c.to_le_bytes();
        let mut batch = Batch::new(self.target);
        batch.push(
            RbbHighFilter::new()
                .with_resistor(u5::from_raw(byte(params.r)))
                .with_rcc(u3::from_raw(byte(params.rcc)))
                .with_capacitor(c0)
                .entry(),
        );
        batch.push(
            RbbLowFilter::new()
                .with_rcc(u3::from_raw(byte(params.rcc)))
                .with_capacitor(u11::from_raw(u16::from_le_bytes([c0, c1])))
                .entry(),
        );
        batch
    }

    /// Selects the transmit baseband's filter and mode (`lms7002m_tbb_path`): the selected
    /// channels' cached `EN_DIR_TBB` follows the mode (not written here); the DAC loopback
    /// forces the bypass.
    pub fn tbb_path(&mut self, path: TbbPath, mode: TbbMode) -> Batch {
        let enabled = mode != TbbMode::Disable;
        let selected = self.banks();
        for (en_dir, _) in self.en_dir.iter_mut().zip(selected).filter(|(_, on)| *on) {
            en_dir.set_tbb(enabled);
        }
        let path = if mode == TbbMode::LoopbackDac {
            TbbPath::Bypass
        } else {
            path
        };
        let loopback = match mode {
            TbbMode::Normal => 0,
            TbbMode::LoopbackDac => 1,
            TbbMode::Disable | TbbMode::Loopback | TbbMode::LoopbackSwapIq => 3,
        };
        let mut batch = Batch::new(self.target);
        batch.push(
            TbbControl::new()
                .with_loopback_swap_iq(mode == TbbMode::LoopbackSwapIq)
                .with_loopback(u2::new(loopback))
                .with_high_filter_powered_down(path != TbbPath::HighBand)
                .with_ladder_filter_powered_down(path != TbbPath::Ladder)
                .with_s5_filter_powered_down(path != TbbPath::Ladder)
                .with_enabled(enabled)
                .entry(),
        );
        batch.push(TbbTest::new().entry());
        batch.push(TbbS5::new().with_bypassed(true).entry());
        batch
    }

    /// Writes the transmit filters' resistor code and the amplifier gain the RC code
    /// implies, plus the cached correction (`lms7002m_tbb_lpf_raw`). libusdr reads channel
    /// A's correction for both ("TODO channel A/B").
    pub fn tbb_lpf(&mut self, params: LpfParams) -> Batch {
        let rcc = byte(params.rcc);
        self.tbb_gain = [rcc; 2];
        // `gc + params.rcc` is unsigned in libusdr: a negative correction larger than the
        // filter code wraps, and clamps to 63.
        let gain = unsigned(self.tbb_gain_correction[0].into())
            .wrapping_add(rcc.into())
            .clamp(1, 63);
        let mut batch = Batch::new(self.target);
        batch.push(
            TbbFilterCal::new()
                .with_high(byte(params.r))
                .with_ladder(byte(params.r))
                .entry(),
        );
        batch.push(amplifier(gain));
        batch
    }

    /// Corrects the transmit amplifier's gain by `correction` codes from the one the filter
    /// implies (`lms7002m_tbb_gain`), on both channels' caches; channel A's filter code is
    /// the base.
    pub fn tbb_gain(&mut self, correction: i8) -> Batch {
        self.tbb_gain_correction = [correction; 2];
        let gain = (i32::from(correction) + i32::from(self.tbb_gain[0])).clamp(1, 63);
        let mut batch = Batch::new(self.target);
        batch.push(amplifier(non_negative(gain)));
        batch
    }
}

/// The transmit amplifier's word for gain code `gain` (1–63) and libusdr's biases.
fn amplifier(gain: u32) -> (Reg, u16) {
    TbbAmplifier::new()
        .with_gain(u6::from_raw(byte(gain)))
        .with_reference_bias(u5::new(12))
        .with_gain_bias(u5::new(12))
        .entry()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_low_band_bandwidth_clamps_the_capacitor_to_zero() {
        // 2.16 GHz / 30 MHz = 72, under 103: libusdr's unsigned subtraction wraps.
        let params = LpfParams::rbb_default(NonZeroU32::new(30_000_000).unwrap(), true);
        assert_eq!((params.c, params.rcc, params.r), (0, 5, 16));
    }

    #[test]
    fn the_tbb_codes_stay_within_their_fields_at_both_ends() {
        for (bandwidth, ladder) in [(0, true), (u32::MAX, true), (0, false), (u32::MAX, false)] {
            let params = LpfParams::tbb_default(bandwidth, ladder);
            assert!(params.r <= 255 && (1..=63).contains(&params.rcc));
        }
    }
}
