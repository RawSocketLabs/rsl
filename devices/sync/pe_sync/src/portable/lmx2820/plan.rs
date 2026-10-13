//! The LMX2820's frequency plan: libusdr's `lmx2820_solver` (`lmx2820_solver_prepare`,
//! `lmx2820_calculate_input_chain`, `lmx2820_tune_vco`,
//! `lmx2820_solver_validate_and_save`) and the register list `lmx2820_tune_internal` writes
//! for it, without SYSREF and instant calibration (not ported).
//!
//! The arithmetic is the C's, step for step: integer widths follow it, and `double`
//! expressions keep its order of operations. `log2`, `ceil` and `floor` of integer
//! quotients take exact integer forms ([`float`](super::super::float)).

use core::fmt;

use bnb::{u2, u3, u4, u5, u6, u9, u11, u12, u15};

use super::super::float::{abs, c_f64, c_u64, ceil_log2_ratio, gcd};
use super::reg::{
    CalibrationClock, ChannelDividers, Control, FeedbackInteger, HighPfdAdjust, LowPfdAdjust,
    MashOrder, Modulator, OutputA, OutputB, OutputSource, ReferenceDivider, ReferenceDoubler,
    ReferenceMultiplier, ReferencePreDivider, Reg, SYSREF_OFF, Sync, VcoStart,
};
use crate::portable::ti_spi::Register;

/// The reference input range (`OSC_IN_MIN`, `OSC_IN_MAX`), in Hz.
const OSC_IN_HZ: core::ops::RangeInclusive<u64> = 5_000_000..=1_400_000_000;
/// The highest reference the doubler takes (`OSC_IN_MAX_DBLR`), in Hz.
const DOUBLER_MAX_HZ: u64 = 250_000_000;
/// The output range (`OUT_FREQ_MIN`, `OUT_FREQ_MAX`), in Hz.
const OUTPUT_HZ: core::ops::RangeInclusive<u64> = 45_000_000..=22_600_000_000;
/// The VCO range (`VCO_MIN`, `VCO_MAX`), in Hz.
const VCO_MIN_HZ: u64 = 5_650_000_000;
/// See [`VCO_MIN_HZ`].
const VCO_MAX_HZ: u64 = 11_300_000_000;
/// The largest pre-R divider (`PLL_R_PRE_DIV_MAX`).
const PRE_DIVIDER_MAX: u32 = 4095;
/// The multiplier's input range (`MULT_IN_FREQ_MIN`, `MULT_IN_FREQ_MAX`), in Hz.
const MULT_IN_MIN_HZ: u64 = 30_000_000;
/// See [`MULT_IN_MIN_HZ`].
const MULT_IN_MAX_HZ: u64 = 70_000_000;
/// The multiplier's output range (`MULT_OUT_FREQ_MIN`, `MULT_OUT_FREQ_MAX`), in Hz.
const MULT_OUT_MIN_HZ: u64 = 180_000_000;
/// See [`MULT_OUT_MIN_HZ`].
const MULT_OUT_MAX_HZ: u64 = 250_000_000;
/// The multiplier's settings (`MULT_MIN` = x3, `MULT_MAX` = x7).
const MULT_MIN: u32 = 3;
/// See [`MULT_MIN`].
const MULT_MAX: u32 = 7;
/// The lowest phase-detector frequency (`FPD_MIN`), in Hz.
const PFD_MIN_HZ: u64 = 5_000_000;
/// The highest, by modulator order (`FPD_MAX`), in Hz.
const PFD_MAX_HZ: [u64; 4] = [400_000_000, 300_000_000, 300_000_000, 250_000_000];
/// The largest post-R divider (`PLL_R_DIV_MAX`).
const DIVIDER_MAX: u32 = 255;
/// The post-R divider's input limit up to a divider of 2 (`PLL_R_DIV_2_IN_FREQ_MAX`), in Hz.
const DIVIDER_IN_MAX_HZ: u64 = 500_000_000;
/// Its limit for larger dividers (`PLL_R_DIV_GT2_IN_FREQ_MAX`), in Hz.
const DIVIDER_IN_MAX_LARGE_HZ: u64 = 250_000_000;
/// The channel dividers' range, as powers of two (`OUT_DIV_LOG2_MIN`, `OUT_DIV_LOG2_MAX`).
const CHANNEL_DIVIDER_LOG2: core::ops::RangeInclusive<u8> = 1..=7;
/// The widest output ratio, as a power of two (`OUT_DIV_DIAP_MAX`).
const RATIO_LOG2_MAX: u32 = 8;
/// The feedback divider's limit (`PLL_N_MAX + 1`, kept in a `uint16_t` by the C).
const FEEDBACK_MAX: u16 = 32_768;

/// The VCO cores (`VCO_CORES`): each one's lower bound, in Hz, and its smallest feedback
/// divider by modulator order. A core covers up to the next one's bound; the last up to
/// [`VCO_MAX_HZ`].
const VCO_CORES: [(u64, [u16; 4]); 7] = [
    (VCO_MIN_HZ, [12, 18, 19, 24]),
    (6_350_000_000, [14, 21, 22, 26]),
    (7_300_000_000, [16, 23, 24, 26]),
    (8_100_000_000, [16, 26, 27, 29]),
    (9_000_000_000, [18, 28, 29, 31]),
    (9_800_000_000, [18, 30, 31, 33]),
    (10_600_000_000, [20, 33, 34, 36]),
];

/// Why the LMX2820 has no configuration for a request (libusdr's `-EINVAL` for each,
/// before any bus access).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlanError {
    /// The reference is outside 5 MHz to 1.4 GHz.
    ReferenceOutOfRange,
    /// An output is outside 45 MHz to 22.6 GHz.
    OutputOutOfRange,
    /// The outputs' ratio is not a power of two, or too wide for the channel dividers.
    OutputRatio,
    /// The VCO falls outside 5.65–11.3 GHz.
    VcoOutOfRange,
    /// The reference path cannot reach the phase detector's range: the multiplier's input
    /// is too low, no multiplier setting lands in its output range, or a divider is out of
    /// range.
    ReferencePath,
    /// The feedback divider is out of range, fractional in integer mode, or its fraction
    /// does not fit 32 bits.
    Feedback,
    /// The VCO or an output misses its frequency.
    Inexact,
}

impl PlanError {
    /// What the error says, for reports.
    pub(crate) const fn reason(self) -> &'static str {
        match self {
            Self::ReferenceOutOfRange => "reference out of range",
            Self::OutputOutOfRange => "output frequency out of range",
            Self::OutputRatio => "output ratio is not a reachable power of two",
            Self::VcoOutOfRange => "VCO out of range",
            Self::ReferencePath => "no reference path reaches the phase detector",
            Self::Feedback => "feedback divider out of range",
            Self::Inexact => "frequency not reached exactly",
        }
    }
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.reason())
    }
}

/// What `lmx2820_tune` is asked for, SYSREF aside.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Request {
    /// The reference, in Hz (`osc_in`).
    pub(crate) reference_hz: u64,
    /// The modulator order (`mash_order`).
    pub(crate) mash: MashOrder,
    /// A forced reference multiplier, 3 to 7 (clamped there, as the C does); `None` lets
    /// the solver decide (`force_mult` = 0).
    pub(crate) multiplier: Option<u8>,
    /// RFOUTA, in Hz.
    pub(crate) out_a_hz: u64,
    /// RFOUTB, in Hz.
    pub(crate) out_b_hz: u64,
}

/// The outputs' VCO, channel dividers and sources (`lmx2820_solver_prepare`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Outputs {
    /// The VCO frequency, in Hz.
    vco_hz: u64,
    /// RFOUTA's channel divider as a power of two, 1 to 7.
    a_log2: u8,
    /// RFOUTB's.
    b_log2: u8,
    /// RFOUTA's source.
    a_source: OutputSource,
    /// RFOUTB's.
    b_source: OutputSource,
}

/// A plan: the reference path, the feedback divider and the outputs (libusdr's
/// `lmx2820_input_chain_t` and `lmx2820_output_chain_t`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Plan {
    /// The reference, in Hz.
    reference_hz: u64,
    /// The reference doubler is on.
    doubled: bool,
    /// The pre-R divider, 1 to 4095.
    pre_divider: u16,
    /// The multiplier: 1 (bypassed) or 3 to 7.
    multiplier: u8,
    /// The post-R divider, 1 to 255.
    divider: u8,
    /// The phase-detector frequency, in Hz.
    pfd_hz: f64,
    /// The modulator order.
    mash: MashOrder,
    /// The VCO core calibration starts from, 1 to 7.
    vco_core: u8,
    /// The feedback divider's integer part.
    n: u16,
    /// Its fraction's numerator.
    num: u32,
    /// Its fraction's denominator.
    den: u32,
    /// The outputs.
    outputs: Outputs,
}

impl Plan {
    /// Plans `request` (`lmx2820_solver` with SYSREF off).
    pub(crate) fn solve(request: &Request) -> Result<Self, PlanError> {
        if !OSC_IN_HZ.contains(&request.reference_hz) {
            return Err(PlanError::ReferenceOutOfRange);
        }
        if !OUTPUT_HZ.contains(&request.out_a_hz) || !OUTPUT_HZ.contains(&request.out_b_hz) {
            return Err(PlanError::OutputOutOfRange);
        }
        let outputs = Outputs::prepare(request.out_a_hz, request.out_b_hz)?;
        let plan = Self::reference_path(request, outputs)?.tune_vco()?;
        plan.validate(request)?;
        Ok(plan)
    }

    /// The reference path to the phase detector (`lmx2820_calculate_input_chain`, up to its
    /// `lmx2820_tune_vco`): the doubler when it fits, otherwise the multiplier or the
    /// dividers the reference needs.
    fn reference_path(request: &Request, outputs: Outputs) -> Result<Self, PlanError> {
        let vco_hz = outputs.vco_hz;
        let (_, min_n) = vco_core(vco_hz, request.mash)?;
        let pfd_max = PFD_MAX_HZ[request.mash as usize].min(vco_hz / u64::from(min_n));
        let pfd_min = PFD_MIN_HZ.max(vco_hz / u64::from(FEEDBACK_MAX));
        let forced = request.multiplier.map_or(0, u32::from);
        let reference = request.reference_hz;
        let need_multiplier = reference < pfd_min || forced != 0;
        let doubled = reference <= DOUBLER_MAX_HZ && reference << 1 <= pfd_max && !need_multiplier;
        let osc_in = reference * if doubled { 2 } else { 1 };

        let (multiplier, pre_divider, divider) = if osc_in < pfd_min || forced != 0 {
            if osc_in < MULT_IN_MIN_HZ {
                return Err(PlanError::ReferencePath);
            }
            // `(unsigned)floor((double)fpd_max / osc_in)`: integer division (`float`).
            let chosen = if forced != 0 {
                forced
            } else {
                u32::try_from(pfd_max / osc_in).unwrap_or(u32::MAX)
            };
            let mut multiplier = chosen.clamp(MULT_MIN, MULT_MAX);
            // `(unsigned)ceil((double)osc_in / MULT_IN_FREQ_MAX)`: `div_ceil` (`float`).
            let pre = if osc_in > MULT_IN_MAX_HZ {
                osc_in.div_ceil(MULT_IN_MAX_HZ)
            } else {
                1
            };
            let at_pre = osc_in / pre;
            let mut mult_out_hz = at_pre * u64::from(multiplier);
            while mult_out_hz < MULT_OUT_MIN_HZ {
                if multiplier == MULT_MAX {
                    return Err(PlanError::ReferencePath);
                }
                multiplier += 1;
                mult_out_hz = at_pre * u64::from(multiplier);
                if mult_out_hz > MULT_OUT_MAX_HZ {
                    return Err(PlanError::ReferencePath);
                }
            }
            while mult_out_hz > MULT_OUT_MAX_HZ {
                if multiplier == MULT_MIN {
                    return Err(PlanError::ReferencePath);
                }
                multiplier -= 1;
                mult_out_hz = at_pre * u64::from(multiplier);
                if mult_out_hz < MULT_OUT_MIN_HZ {
                    return Err(PlanError::ReferencePath);
                }
            }
            let divider = if mult_out_hz > pfd_max {
                mult_out_hz.div_ceil(pfd_max)
            } else {
                1
            };
            (multiplier, pre, divider)
        } else if osc_in > pfd_max {
            let total = osc_in.div_ceil(pfd_max);
            if total > u64::from(PRE_DIVIDER_MAX * DIVIDER_MAX) {
                return Err(PlanError::ReferencePath);
            }
            if total <= u64::from(PRE_DIVIDER_MAX) {
                (1, total, 1)
            } else {
                let pre = u64::from(PRE_DIVIDER_MAX);
                (1, pre, total.div_ceil(pre))
            }
        } else {
            (1, 1, 1)
        };
        // The C narrows each into its `uint8_t` or `uint16_t` field after these checks.
        let (Ok(divider), Ok(pre_divider)) = (u8::try_from(divider), u16::try_from(pre_divider))
        else {
            return Err(PlanError::ReferencePath);
        };
        let multiplier = u8::try_from(multiplier).map_err(|_| PlanError::ReferencePath)?;
        let into_divider = osc_in * u64::from(multiplier) / u64::from(pre_divider);
        let divider_in_max = if divider <= 2 {
            DIVIDER_IN_MAX_HZ
        } else {
            DIVIDER_IN_MAX_LARGE_HZ
        };
        if into_divider > divider_in_max {
            return Err(PlanError::ReferencePath);
        }
        let pfd_hz = c_f64(osc_in) * f64::from(multiplier)
            / f64::from(u32::from(pre_divider) * u32::from(divider));
        if pfd_hz < c_f64(pfd_min) || pfd_hz > c_f64(pfd_max) {
            return Err(PlanError::ReferencePath);
        }
        Ok(Self {
            reference_hz: reference,
            doubled,
            pre_divider,
            multiplier,
            divider,
            pfd_hz,
            mash: request.mash,
            vco_core: 0,
            n: 0,
            num: 0,
            den: 0,
            outputs,
        })
    }

    /// The feedback divider for the VCO (`lmx2820_tune_vco`): `n + num / den` over the
    /// phase detector, the fraction reduced, which must land on the VCO exactly.
    #[expect(
        clippy::float_cmp,
        reason = "libusdr's integer-mode and exactness checks compare doubles exactly"
    )]
    fn tune_vco(mut self) -> Result<Self, PlanError> {
        let vco_hz = self.outputs.vco_hz;
        let (core, min_n) = vco_core(vco_hz, self.mash)?;
        let n_total = c_f64(vco_hz) / self.pfd_hz;
        if n_total < f64::from(min_n) || n_total > f64::from(FEEDBACK_MAX) {
            return Err(PlanError::Feedback);
        }
        // `(uint16_t)n_total`: at most 32768 here.
        let n = u16::try_from(c_u64(n_total)).map_err(|_| PlanError::Feedback)?;
        let fraction = n_total - f64::from(n);
        if f64::from(n) != n_total && self.mash == MashOrder::Integer {
            return Err(PlanError::Feedback);
        }
        let r_div = u32::from(self.divider) * u32::from(self.pre_divider);
        let mut den = c_u64(self.pfd_hz * f64::from(r_div));
        let mut num = c_u64(fraction * c_f64(den) + 0.5);
        let divisor = gcd(num, den);
        if divisor > 1 {
            num /= divisor;
            den /= divisor;
        }
        let (Ok(num), Ok(den)) = (u32::try_from(num), u32::try_from(den)) else {
            return Err(PlanError::Feedback);
        };
        let ff = c_f64(self.reference_hz)
            * f64::from(if self.doubled { 2 } else { 1 })
            * f64::from(self.multiplier);
        let reached = ff * f64::from(n) / f64::from(r_div)
            + ff * f64::from(num) / f64::from(r_div) / f64::from(den);
        if reached != c_f64(vco_hz) {
            return Err(PlanError::Inexact);
        }
        self.vco_core = core;
        self.n = n;
        self.num = num;
        self.den = den;
        Ok(self)
    }

    /// Both outputs land exactly on their requests (`lmx2820_solver_validate_and_save`).
    fn validate(&self, request: &Request) -> Result<(), PlanError> {
        // The VCO the fraction reaches, which `tune_vco` proved equal to the request.
        let vco = c_f64(self.outputs.vco_hz);
        let reached = |source, log2: u8| match source {
            OutputSource::VcoDoubler => vco * 2.0,
            OutputSource::Vco => vco,
            OutputSource::ChannelDivider => vco / f64::from(1_u32 << log2),
        };
        let a = reached(self.outputs.a_source, self.outputs.a_log2);
        let b = reached(self.outputs.b_source, self.outputs.b_log2);
        // `RF_ACCURACY` is 0.
        if abs(c_f64(request.out_a_hz) - a) > 0.0 || abs(c_f64(request.out_b_hz) - b) > 0.0 {
            return Err(PlanError::Inexact);
        }
        Ok(())
    }

    /// The registers `lmx2820_tune_internal` writes, in its order, ending with R0, whose
    /// `FCAL_EN` starts the VCO calibration.
    pub(crate) fn registers(&self) -> [(u8, u16); 25] {
        let [num_high, num_low] = split(self.num);
        let [den_high, den_low] = split(self.den);
        let [
            sysref_output,
            phase_sync_input,
            sysref_pulses,
            sysref_delay,
            sysref_divider,
            sysref,
        ] = SYSREF_OFF.map(|(reg, value)| (u8::from(reg), value));
        let raw = |reg: Reg, value| (u8::from(reg), value);
        [
            OutputB::new()
                .with_source(self.outputs.b_source)
                .with_a_power(u3::new(7))
                .entry(),
            OutputA::new().with_source(self.outputs.a_source).entry(),
            sysref_output,
            phase_sync_input,
            sysref_pulses,
            sysref_delay,
            sysref_divider,
            sysref,
            // No instant calibration: its numerator is 0.
            raw(Reg::InstantNumeratorLow, 0),
            raw(Reg::InstantNumeratorHigh, 0),
            raw(Reg::NumeratorLow, num_low),
            raw(Reg::NumeratorHigh, num_high),
            raw(Reg::DenominatorLow, den_low),
            raw(Reg::DenominatorHigh, den_high),
            FeedbackInteger::new().with_n(u15::new(self.n)).entry(),
            Modulator::new()
                .with_reserved(u3::new(1))
                .with_running(true)
                .with_order(self.mash)
                .entry(),
            ChannelDividers::new()
                .with_reserved_high(u4::new(1))
                .with_b_log2_less_one(u3::new(self.outputs.b_log2 - 1))
                .with_a_log2_less_one(u3::new(self.outputs.a_log2 - 1))
                .with_reserved_low(u6::new(1))
                .entry(),
            VcoStart::new()
                .with_core(u3::new(self.vco_core))
                .with_reserved(u5::new(2))
                .with_sub_band(0xbf)
                .entry(),
            ReferencePreDivider::new()
                .with_reserved(u4::new(3))
                .with_divider(u12::new(self.pre_divider))
                .entry(),
            ReferenceDivider::new()
                .with_divider(self.divider)
                .with_reserved(u5::new(0x18))
                .entry(),
            ReferenceMultiplier::new()
                .with_multiplier(u3::new(self.multiplier))
                .with_reserved(bnb::u10::new(8))
                .entry(),
            ReferenceDoubler::new()
                .with_reserved_high(u11::new(0x30))
                .with_doubled(self.doubled)
                .with_reserved_low(u4::new(3))
                .entry(),
            CalibrationClock::new()
                .with_reserved(true)
                .with_divider_log2(calibration_clock_divider(self.reference_hz))
                .with_instant_delay(u11::new(0x1f4))
                .entry(),
            Sync::new()
                .with_reserved(u9::new(0x15e))
                .with_vtune_lock_detect(true)
                .entry(),
            Control::new()
                .with_reserved_high(u2::new(1))
                .with_skip_amplitude_calibration(true)
                .with_high_pfd_adjust(high_pfd_adjust(self.pfd_hz))
                .with_low_pfd_adjust(low_pfd_adjust(self.pfd_hz))
                .with_doubler_calibration(true)
                .with_reserved_low(true)
                .with_calibrate(true)
                .entry(),
        ]
    }
}

impl Outputs {
    /// The VCO and how each output reaches it (`lmx2820_solver_prepare`): the outputs'
    /// ratio must be a power of two; the faster one sets the VCO, directly, through the
    /// doubler above the VCO's range, or through a channel divider below it.
    fn prepare(a_hz: u64, b_hz: u64) -> Result<Self, PlanError> {
        // `rf_max` is A only when A is strictly faster.
        let (max_hz, min_hz) = if a_hz > b_hz {
            (a_hz, b_hz)
        } else {
            (b_hz, a_hz)
        };
        // `log2(max / min)` within 1e-8 of a whole number: only an exact power of two can
        // pass the solver's later exactness checks, so anything else is refused here
        // rather than there (both before any bus access).
        let ratio = max_hz / min_hz;
        if max_hz % min_hz != 0 || !ratio.is_power_of_two() {
            return Err(PlanError::OutputRatio);
        }
        let ratio_log2 = ratio.trailing_zeros();
        if ratio_log2 > RATIO_LOG2_MAX {
            return Err(PlanError::OutputRatio);
        }
        let narrow = |log2: u32| u8::try_from(log2).map_err(|_| PlanError::OutputRatio);
        let (vco_hz, max_source, min_source, max_log2, min_log2) = if max_hz > VCO_MAX_HZ {
            // `(uint64_t)((double)rf_max / 2 + 0.5)`: halving and adding a half are exact.
            let vco_hz = max_hz.div_ceil(2);
            match ratio_log2 {
                0 => (
                    vco_hz,
                    OutputSource::VcoDoubler,
                    OutputSource::VcoDoubler,
                    1,
                    1,
                ),
                1 => (vco_hz, OutputSource::VcoDoubler, OutputSource::Vco, 1, 1),
                _ => {
                    let min_log2 = narrow(ratio_log2 - 1)?;
                    let max_log2 = if min_log2 == *CHANNEL_DIVIDER_LOG2.end() {
                        min_log2
                    } else {
                        1
                    };
                    let source = OutputSource::ChannelDivider;
                    (vco_hz, OutputSource::VcoDoubler, source, max_log2, min_log2)
                }
            }
        } else if max_hz < VCO_MIN_HZ {
            if ratio_log2 > RATIO_LOG2_MAX - 2 {
                return Err(PlanError::OutputRatio);
            }
            let start = (max_hz << 1).max(VCO_MIN_HZ);
            // `(uint8_t)ceil(log2((double)vco / rf_max))`, at least 1 (`float`).
            let max_log2 = narrow(ceil_log2_ratio(start, max_hz).max(1))?;
            let min_log2 = max_log2 + narrow(ratio_log2)?;
            let top = *CHANNEL_DIVIDER_LOG2.end();
            if !CHANNEL_DIVIDER_LOG2.contains(&max_log2)
                || !CHANNEL_DIVIDER_LOG2.contains(&min_log2)
                || (min_log2 == top || max_log2 == top) && min_log2 != max_log2
            {
                return Err(PlanError::OutputRatio);
            }
            let source = OutputSource::ChannelDivider;
            (max_hz << max_log2, source, source, max_log2, min_log2)
        } else {
            if ratio_log2 > RATIO_LOG2_MAX - 1 {
                return Err(PlanError::OutputRatio);
            }
            if ratio_log2 == 0 {
                (max_hz, OutputSource::Vco, OutputSource::Vco, 1, 1)
            } else {
                let min_log2 = narrow(ratio_log2)?;
                let max_log2 = if min_log2 == *CHANNEL_DIVIDER_LOG2.end() {
                    min_log2
                } else {
                    1
                };
                let source = OutputSource::ChannelDivider;
                (max_hz, OutputSource::Vco, source, max_log2, min_log2)
            }
        };
        // `rf_min` is A when the two are equal.
        let a_is_min = a_hz <= b_hz;
        let (a_log2, a_source, b_log2, b_source) = if a_is_min {
            (min_log2, min_source, max_log2, max_source)
        } else {
            (max_log2, max_source, min_log2, min_source)
        };
        if !CHANNEL_DIVIDER_LOG2.contains(&a_log2) || !CHANNEL_DIVIDER_LOG2.contains(&b_log2) {
            return Err(PlanError::OutputRatio);
        }
        Ok(Self {
            vco_hz,
            a_log2,
            b_log2,
            a_source,
            b_source,
        })
    }
}

/// The VCO core holding `vco_hz` and its smallest feedback divider for `mash`
/// (`lmx2820_get_worst_vco_core`).
fn vco_core(vco_hz: u64, mash: MashOrder) -> Result<(u8, u16), PlanError> {
    if !(VCO_MIN_HZ..=VCO_MAX_HZ).contains(&vco_hz) {
        return Err(PlanError::VcoOutOfRange);
    }
    let index = VCO_CORES
        .iter()
        .rposition(|&(low, _)| vco_hz >= low)
        .ok_or(PlanError::VcoOutOfRange)?;
    let core = u8::try_from(index + 1).map_err(|_| PlanError::VcoOutOfRange)?;
    Ok((core, VCO_CORES[index].1[mash as usize]))
}

/// `FCAL_HPFD_ADJ` for the phase-detector frequency.
fn high_pfd_adjust(pfd_hz: f64) -> HighPfdAdjust {
    if pfd_hz <= 100e6 {
        HighPfdAdjust::UpTo100MHz
    } else if pfd_hz <= 150e6 {
        HighPfdAdjust::UpTo150MHz
    } else if pfd_hz <= 200e6 {
        HighPfdAdjust::UpTo200MHz
    } else {
        HighPfdAdjust::Above200MHz
    }
}

/// `FCAL_LPFD_ADJ` for the phase-detector frequency.
fn low_pfd_adjust(pfd_hz: f64) -> LowPfdAdjust {
    if pfd_hz < 2.5e6 {
        LowPfdAdjust::Below2_5MHz
    } else if pfd_hz < 5e6 {
        LowPfdAdjust::From2_5MHz
    } else if pfd_hz < 10e6 {
        LowPfdAdjust::From5MHz
    } else {
        LowPfdAdjust::From10MHz
    }
}

/// `CAL_CLK_DIV` for the reference as requested (before the doubler).
fn calibration_clock_divider(reference_hz: u64) -> u3 {
    u3::new(if reference_hz <= 200_000_000 {
        0
    } else if reference_hz <= 400_000_000 {
        1
    } else if reference_hz <= 800_000_000 {
        2
    } else {
        3
    })
}

/// A 32-bit number as its upper and lower 16-bit halves.
fn split(value: u32) -> [u16; 2] {
    let [a, b, c, d] = value.to_be_bytes();
    [u16::from_be_bytes([a, b]), u16::from_be_bytes([c, d])]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `PE_SYNC`'s LMX2820 #0: 150 MHz from the LMK05318B, second-order modulator, 1 GHz on
    /// both outputs.
    const SYNTH0: Request = Request {
        reference_hz: 150_000_000,
        mash: MashOrder::Second,
        multiplier: None,
        out_a_hz: 1_000_000_000,
        out_b_hz: 1_000_000_000,
    };

    #[test]
    fn synthesizer_0s_registers_match_libusdrs() {
        // The oracle's golden session.trace, lines 1349-1373: what libusdr wrote for SYNTH0.
        // 300 MHz (doubled) into the phase detector; 8 GHz is 26 + 2/3 of it, on VCO core 3;
        // both outputs divide by 8.
        let words = Plan::solve(&SYNTH0).expect("libusdr tunes it").registers();
        let golden: [(u8, u16); 25] = [
            (0x4f, 0x000e),
            (0x4e, 0x0000),
            (0x45, 0x0011),
            (0x44, 0x0000),
            (0x43, 0x0000),
            (0x42, 0x003f),
            (0x41, 0x0001),
            (0x40, 0x4080),
            (0x2d, 0x0000),
            (0x2c, 0x0000),
            (0x2b, 0x0002),
            (0x2a, 0x0000),
            (0x27, 0x0003),
            (0x26, 0x0000),
            (0x24, 0x001a),
            (0x23, 0x3100),
            (0x20, 0x1481),
            (0x16, 0x62bf),
            (0x0e, 0x3001),
            (0x0d, 0x0038),
            (0x0c, 0x0408),
            (0x0b, 0x0613),
            (0x02, 0x83e8),
            (0x01, 0x57a0),
            (0x00, 0x6670),
        ];
        assert_eq!(words, golden);
    }

    #[test]
    fn synthesizer_1s_registers_match_libusdrs() {
        // Golden lines 1388-1412: 550 MHz needs 8.8 GHz (VCO core 4) over divide-by-16,
        // 29 + 1/3 of the same 300 MHz.
        let request = Request {
            out_a_hz: 550_000_000,
            out_b_hz: 550_000_000,
            ..SYNTH0
        };
        let words = Plan::solve(&request).expect("libusdr tunes it").registers();
        let specific = [
            (0x2b, 0x0001),
            (0x2a, 0x0000),
            (0x27, 0x0003),
            (0x26, 0x0000),
            (0x24, 0x001d),
            (0x23, 0x3100),
            (0x20, 0x16c1),
            (0x16, 0x82bf),
        ];
        assert_eq!(words[..2], [(0x4f, 0x000e), (0x4e, 0x0000)]);
        assert_eq!(words[10..18], specific);
        // SYSREF off (R69 to R64), no instant calibration (R45, R44) and the same reference
        // path (R14 to R0) as synthesizer 0.
        let synth0 = Plan::solve(&SYNTH0).unwrap().registers();
        assert_eq!(words[2..10], synth0[2..10]);
        assert_eq!(words[18..], synth0[18..]);
    }

    #[test]
    fn outputs_apart_by_other_than_a_power_of_two_are_refused() {
        let request = Request {
            out_b_hz: 300_000_000,
            ..SYNTH0
        };
        assert_eq!(Plan::solve(&request), Err(PlanError::OutputRatio));
    }

    #[test]
    fn a_forced_multiplier_below_its_input_range_is_refused() {
        // Forcing the multiplier turns the doubler off; 10 MHz is below its 30 MHz input.
        let request = Request {
            reference_hz: 10_000_000,
            multiplier: Some(3),
            ..SYNTH0
        };
        assert_eq!(Plan::solve(&request), Err(PlanError::ReferencePath));
    }
}
