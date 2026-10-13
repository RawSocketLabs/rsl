//! The LMK05318's frequency plan as numbers, without registers: libusdr's XO setup
//! (`lmk05318_set_xo_fref`), APLL1 tuning without the DPLL (`lmk05318_tune_apll1`), the
//! output solver (`lmk05318_solver`, `lmk05318_solver_helper`, `lmk05318_calc_vco2_div`),
//! APLL2's settling (`lmk05318_tune_apll2`) and output 7's two-stage divider
//! (`lmk05318_set_out_div`, `lmk05318_factorize_out7div`).
//!
//! # Arithmetic
//!
//! The arithmetic is libusdr's, operation for operation, so register values match bit for
//! bit. Integer widths follow the C (32-bit `unsigned` sums wrap where the C's do); float
//! steps keep the C's order and its `double` promotions. The solver's functions are
//! compiled `optimize("-Ofast")` in libusdr, and GCC (libusdr's compiler: the attribute is
//! GCC's) then changes two expressions, reproduced here as the pinned oracle build's
//! disassembly shows them:
//!
//! - `calc_vco2_div` divides by `VCO_APLL1` as a multiplication by its rounded reciprocal
//!   ([`VCO1_RECIPROCAL`]); for the integer VCO frequencies the solver scans, the
//!   difference never reaches its results, but the C's form is kept;
//! - `fvco2 / pd / div` (an output's resulting frequency) becomes `fvco2 / (div * pd)`,
//!   which does change the last bit of some results (`oracle/tests/lmk05318_parity.rs`).
//!
//! Everything else in those functions compiles as written. Code outside them (XO, APLL1,
//! APLL2 settling, output 7) is plain C.
//!
//! The reference is therefore libusdr as GCC builds it for x86-64 (SSE2 `double`, no fused
//! multiply-add: the oracle passes `-ffp-contract=off`, and Rust never fuses); the oracle
//! refuses to build with another C compiler. Another compiler or target can change the
//! C's last bits, not this port's.
//!
//! The module is self-contained but for [`gcd`], so the oracle's parity tests compile it
//! directly and compare its plans with libusdr's.
//!
//! # Not ported
//!
//! The DPLL (`lmk05318_dpll_config` and the DPLL halves of APLL1 tuning and the common
//! registers): neither front end enables it, so APLL1 always runs free with a programmed
//! 24-bit denominator.

use crate::cal::search::gcd;

/// APLL1's VCO, fixed by the chip's BAW resonator (`VCO_APLL1`), in Hz.
pub(crate) const VCO1_HZ: u64 = 2_500_000_000;
/// [`VCO1_HZ`] as the C converts it to `double` (exactly).
const VCO1_F64: f64 = 2_500_000_000.0;
/// `1 / VCO_APLL1` rounded to `f64`: GCC's `-Ofast` replaces `calc_vco2_div`'s division by
/// `VCO_APLL1` with a multiplication by this.
pub(crate) const VCO1_RECIPROCAL: f64 = 1.0 / 2_500_000_000.0;
/// APLL2's VCO range (`VCO_APLL2_MIN`, `VCO_APLL2_MAX`), in Hz.
const VCO2_MIN_HZ: u64 = 5_500_000_000;
/// See [`VCO2_MIN_HZ`].
const VCO2_MAX_HZ: u64 = 6_250_000_000;
/// APLL1's phase-detector range (`APLL1_PD_MIN`, `APLL1_PD_MAX`), in Hz.
const APLL1_PD_MIN_HZ: u32 = 1_000_000;
/// See [`APLL1_PD_MIN_HZ`].
const APLL1_PD_MAX_HZ: u32 = 80_000_000;
/// APLL2's phase-detector range (`APLL2_PD_MIN`, `APLL2_PD_MAX`), in Hz.
const APLL2_PD_MIN_HZ: u32 = 10_000_000;
/// See [`APLL2_PD_MIN_HZ`].
const APLL2_PD_MAX_HZ: u32 = 150_000_000;
/// The XO frequencies libusdr accepts (`XO_FREF_MIN`, `XO_FREF_MAX`), in Hz.
const XO_MIN_HZ: u32 = 10_000_000;
/// See [`XO_MIN_HZ`].
const XO_MAX_HZ: u32 = 100_000_000;
/// APLL1's largest reference divider (`APLL1_DIVIDER_MAX`).
const APLL1_DIVIDER_MAX: u32 = 32;
/// APLL2's post-dividers, 2 to 7 (`APLL2_PDIV_MIN`, `APLL2_PDIV_MAX`).
const APLL2_POST_DIVIDERS: core::ops::RangeInclusive<u32> = 2..=7;
/// Output 7's first stage, at most 256 (`OUTDIV7_STAGE1_MAX`).
const OUT7_STAGE1_MAX: u16 = 256;
/// Output 7's first stage when a second follows, at least 6 (`OUTDIV7_STAGE1_WITH_ST2_MIN`).
const OUT7_STAGE1_WITH_STAGE2_MIN: u16 = 6;
/// Output 7's second stage, at most 2^24 (`OUTDIV7_STAGE2_MAX`).
const OUT7_STAGE2_MAX: u64 = 1 << 24;
/// The tolerance libusdr requests for every output, either side (`LMK05318_FREQ_DELTA`),
/// in Hz.
const FREQ_DELTA_HZ: u32 = 2;
/// The output slots the solver works on: outputs 0 and 1 share a divider, as do 2 and 3
/// (`LMK05318_MAX_REAL_PORTS`).
const SLOTS: usize = 6;

/// Why the LMK05318 clock synthesizer of a front end has no configuration for a request
/// (libusdr's `-EINVAL` for each).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ClockPlanError {
    /// The XO frequency is outside 10–100 MHz, or its phase-detector frequency outside
    /// APLL1's 1–80 MHz.
    #[error("LMK05318 XO frequency out of range")]
    XoOutOfRange,
    /// No BAW lock-detection settings exist for this XO (12.8, 25, 26 or 52 MHz only).
    #[error("LMK05318 has no BAW lock settings for this XO")]
    UnsupportedXo,
    /// APLL1's fraction does not fit 24 bits or misses the VCO by more than 0.1 mHz.
    #[error("LMK05318 APLL1 cannot reach its VCO exactly")]
    Apll1Inexact,
    /// No requests, more than eight, or an output number past 7.
    #[error("LMK05318 output request malformed")]
    BadRequest,
    /// An LVCMOS format on outputs 0 to 3, which only drive differential formats.
    #[error("LMK05318 outputs 0 to 3 cannot drive LVCMOS")]
    LvcmosOnLowOutput,
    /// Two requests for one divider (the same output, or 0 and 1, or 2 and 3) differ.
    #[error("LMK05318 output requests sharing a divider differ")]
    ConflictingRequests,
    /// An inverted output that APLL1 cannot reach; only APLL1 inverts.
    #[error("LMK05318 can invert only outputs APLL1 drives")]
    InversionNeedsApll1,
    /// A frequency no APLL2 post-divider and output divider can reach.
    #[error("LMK05318 output frequency out of range")]
    OutOfRange,
    /// No APLL2 VCO frequency serves every output the solver routed to it.
    #[error("no LMK05318 APLL2 plan serves every output")]
    NoSolution,
    /// A solved output misses its frequency by more than its tolerance.
    #[error("LMK05318 output frequency missed")]
    Inexact,
    /// APLL2's phase-detector frequency is outside 10–150 MHz.
    #[error("LMK05318 APLL2 phase detector out of range")]
    Apll2PhaseDetector,
    /// An output divider beyond its output's range.
    #[error("LMK05318 output divider out of range")]
    BadDivider,
}

/// The XO input buffer's type (`enum xo_input_type`, the YAML's `x-in-opts`).
#[allow(
    dead_code,
    reason = "libusdr's full set; the front ends use two, the oracle's plan tests all"
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum XoInput {
    /// DC-coupled differential, external termination (`XO_DC_DIFF_EXT`).
    DcDifferentialExternal,
    /// AC-coupled differential, external termination (`XO_AC_DIFF_EXT`).
    AcDifferentialExternal,
    /// AC-coupled differential, internal 100 Ω termination (`XO_AC_DIFF_INT_100`).
    AcDifferentialInternal100,
    /// HCSL, internal 50 Ω termination (`XO_HCSL_INT_50`).
    HcslInternal50,
    /// LVCMOS (`XO_CMOS`).
    Cmos,
    /// Single-ended, internal 50 Ω termination (`XO_SE_INT_50`).
    SingleEndedInternal50,
}

/// The XO, as `lmk05318_create` takes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Xo {
    /// Its frequency, in Hz.
    pub(crate) hz: u32,
    /// Its input buffer.
    pub(crate) input: XoInput,
    /// Ignore the XO frequency detector (`xo_fdet_bypass`).
    pub(crate) detector_bypass: bool,
}

/// How the XO reaches APLL1's phase detector: through the doubler or a divider, never both
/// (`pll1_fref_rdiv`, `doubler_enabled`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct XoPath {
    /// The doubler is on.
    pub(crate) doubler: bool,
    /// The reference divider, 1 to 32.
    pub(crate) divider: u32,
}

/// A sigma-delta modulator order libusdr picks (`SDM_ORDER_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SdmOrder {
    /// Integer division: no fraction.
    Integer = 0,
    /// Third order, for a fractional divider.
    Third = 3,
}

impl SdmOrder {
    /// The order a divider with numerator `num` needs.
    const fn for_numerator(num: u64) -> Self {
        if num == 0 { Self::Integer } else { Self::Third }
    }
}

/// APLL1's feedback divider without the DPLL: `n + num / den`, with a programmed 24-bit
/// denominator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Apll1Plan {
    /// Integer part, at most 65535.
    pub(crate) n: u32,
    /// Fraction numerator, 24 bits.
    pub(crate) num: u64,
    /// Fraction denominator, 24 bits.
    pub(crate) den: u64,
    /// The modulator order.
    pub(crate) order: SdmOrder,
}

impl Xo {
    /// The XO path (`lmk05318_set_xo_fref`): the doubler when twice the XO stays within
    /// APLL1's phase detector, otherwise the smallest divider that does.
    pub(crate) fn path(self) -> Result<XoPath, ClockPlanError> {
        if self.hz < XO_MIN_HZ || self.hz > XO_MAX_HZ {
            return Err(ClockPlanError::XoOutOfRange);
        }
        let path = if self.hz.wrapping_mul(2) <= APLL1_PD_MAX_HZ {
            XoPath {
                doubler: true,
                divider: 1,
            }
        } else {
            let divider = (f64::from(self.hz) / f64::from(APLL1_PD_MAX_HZ)).ceil();
            XoPath {
                doubler: false,
                // The C converts the double to `unsigned`; the value is at most 2 here.
                divider: c_u32(c_u64(divider)),
            }
        };
        if path.divider > APLL1_DIVIDER_MAX {
            return Err(ClockPlanError::XoOutOfRange);
        }
        Ok(path)
    }

    /// APLL1's divider for this XO without the DPLL (`lmk05318_tune_apll1`): the fraction
    /// over the phase-detector frequency, reduced, which must land on the VCO within
    /// 0.1 mHz.
    pub(crate) fn apll1(self, path: XoPath) -> Result<Apll1Plan, ClockPlanError> {
        let doubling = if path.doubler { 2.0_f32 } else { 1.0_f32 };
        let fpd1 = (f64::from(self.hz) / f64::from(path.divider)) * f64::from(doubling);
        if fpd1 < f64::from(APLL1_PD_MIN_HZ) || fpd1 > f64::from(APLL1_PD_MAX_HZ) {
            return Err(ClockPlanError::XoOutOfRange);
        }
        let r = VCO1_F64 / fpd1;
        let n = c_unsigned(r);
        let fraction = r - f64::from(n);
        if n > u32::from(u16::MAX) {
            return Err(ClockPlanError::Apll1Inexact);
        }
        let mut den = u64::from(self.hz.wrapping_mul(if path.doubler { 2 } else { 1 }));
        let mut num = c_u64(fraction * c_f64(den) + 0.5);
        let divisor = gcd(num, den);
        if divisor > 1 {
            num /= divisor;
            den /= divisor;
        }
        if den > (1 << 24) - 1 {
            return Err(ClockPlanError::Apll1Inexact);
        }
        let vco = fpd1 * (f64::from(n) + c_f64(num) / c_f64(den));
        if (vco - VCO1_F64).abs() > 1e-4 {
            return Err(ClockPlanError::Apll1Inexact);
        }
        Ok(Apll1Plan {
            n,
            num,
            den,
            order: SdmOrder::for_numerator(num),
        })
    }
}

/// An output's driver format (`enum lmk05318_type`).
#[allow(
    dead_code,
    reason = "libusdr's full set; the front ends use three, the oracle's plan tests all"
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutputFormat {
    /// Off (`OUT_OFF`).
    Off,
    /// AC-coupled LVDS (`LVDS`).
    Lvds,
    /// AC-coupled CML (`CML`).
    Cml,
    /// AC-coupled LVPECL (`LVPECL`).
    Lvpecl,
    /// HCSL, external 50 Ω termination (`HCSL_EXT_50`).
    HcslExternal50,
    /// HCSL, internal 50 Ω termination (`HCSL_INT_50`).
    HcslInternal50,
    /// LVCMOS, both pins high-impedance (`LVCMOS_HIZ_HIZ`); this and the LVCMOS formats
    /// below need outputs 4 to 7.
    LvcmosHizHiz,
    /// LVCMOS: P high-impedance, N inverted (`LVCMOS_HIZ_N`).
    LvcmosHizN,
    /// LVCMOS: P high-impedance, N in phase (`LVCMOS_HIZ_P`).
    LvcmosHizP,
    /// LVCMOS, both pins low (`LVCMOS_LOW_LOW`).
    LvcmosLowLow,
    /// LVCMOS: P inverted, N high-impedance (`LVCMOS_N_HIZ`).
    LvcmosNHiz,
    /// LVCMOS: both inverted (`LVCMOS_N_N`).
    LvcmosNN,
    /// LVCMOS: P inverted, N in phase (`LVCMOS_N_P`).
    LvcmosNP,
    /// LVCMOS: P in phase, N high-impedance (`LVCMOS_P_HIZ`).
    LvcmosPHiz,
    /// LVCMOS: P in phase, N inverted (`LVCMOS_P_N`).
    LvcmosPN,
    /// LVCMOS: both in phase (`LVCMOS_P_P`).
    LvcmosPP,
}

impl OutputFormat {
    /// An LVCMOS format, which outputs 0 to 3 cannot drive (the C's `type > HCSL_INT_50`).
    const fn is_lvcmos(self) -> bool {
        (self as u8) > (Self::HcslInternal50 as u8)
    }
}

/// Which APLL may drive an output (`enum lmk05318_port_affinity`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Affinity {
    /// APLL1 when it reaches the frequency exactly, otherwise APLL2 (`AFF_ANY`).
    #[default]
    Any,
    /// APLL1 only (`AFF_APLL1`).
    Apll1,
    /// APLL2 only (`AFF_APLL2`).
    Apll2,
}

/// The clock an output's mux selects (`enum lmk05318_out_pll_sel_t`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Mux {
    /// APLL1 (`OUT_PLL_SEL_APLL1_P1`).
    #[default]
    Apll1 = 0,
    /// APLL1, inverted (`OUT_PLL_SEL_APLL1_P1_INV`).
    Apll1Inverted = 1,
    /// APLL2 through post-divider 1 (`OUT_PLL_SEL_APLL2_P1`).
    Apll2Post1 = 2,
    /// APLL2 through post-divider 2 (`OUT_PLL_SEL_APLL2_P2`).
    Apll2Post2 = 3,
}

/// What one output should carry (`lmk05318_port_request`, `lmk05318_set_port_affinity`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OutputRequest {
    /// The output, 0 to 7.
    pub(crate) port: u8,
    /// Its frequency in Hz; 0 turns it off.
    pub(crate) hz: u32,
    /// How far above `hz` it may land, in Hz.
    pub(crate) above_hz: u32,
    /// How far below `hz` it may land, in Hz.
    pub(crate) below_hz: u32,
    /// Invert it; only APLL1 can.
    pub(crate) inverted: bool,
    /// Its driver format.
    pub(crate) format: OutputFormat,
    /// Which APLL may drive it.
    pub(crate) affinity: Affinity,
}

impl OutputRequest {
    /// Output `port` at `hz` in `format`, within libusdr's 2 Hz either side, from either
    /// APLL (`lmk05318_port_request`).
    pub(crate) const fn new(port: u8, hz: u32, format: OutputFormat) -> Self {
        Self {
            port,
            hz,
            above_hz: FREQ_DELTA_HZ,
            below_hz: FREQ_DELTA_HZ,
            inverted: false,
            format,
            affinity: Affinity::Any,
        }
    }

    /// The same request, restricted to `affinity` (`lmk05318_set_port_affinity`).
    pub(crate) const fn with_affinity(mut self, affinity: Affinity) -> Self {
        self.affinity = affinity;
        self
    }

    /// The frequencies the output may land on, in Hz (`lmk05318_get_freq_range`): the C
    /// sums in 32 bits, so the upper end wraps there.
    const fn range(&self) -> (u32, u32) {
        let low = if self.hz >= self.below_hz {
            self.hz - self.below_hz
        } else {
            1
        };
        (low, self.hz.wrapping_add(self.above_hz))
    }
}

/// How the solver reaches an output.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Route {
    /// The clock the output's mux selects.
    pub(crate) mux: Mux,
    /// The output divider.
    pub(crate) divider: u64,
    /// The frequency that results, in Hz.
    pub(crate) hz: f64,
}

/// One output of a plan, in output order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OutputPlan {
    /// The request.
    pub(crate) request: OutputRequest,
    /// How it is reached; `None` for an output turned off.
    pub(crate) route: Option<Route>,
}

/// APLL2's settings once a plan is made (`lmk05318_tune_apll2`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Apll2Setting {
    /// Powered down: no output uses it, or its last plan is out of range.
    Off,
    /// Running.
    On {
        /// Feedback divider integer part.
        n: u32,
        /// Feedback fraction numerator, 24 bits.
        num: u32,
        /// Feedback fraction denominator, 24 bits.
        den: u32,
        /// Post-divider 1, 2 to 7.
        pd1: u32,
        /// Post-divider 2, 2 to 7.
        pd2: u32,
        /// The modulator order.
        order: SdmOrder,
    },
}

/// A plan: APLL2's settings and every requested output, in output order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Plan {
    /// APLL2.
    pub(crate) apll2: Apll2Setting,
    /// The outputs.
    pub(crate) outputs: Vec<OutputPlan>,
}

/// APLL2's state between plans, cascaded from APLL1 (the APLL2 fields of
/// `lmk05318_state_t`). A plan that routes nothing to APLL2 keeps the last one's numbers,
/// as libusdr does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Apll2 {
    /// Primary reference divider, 3 to 6 (`fref_pll2_div_rp`).
    pub(crate) primary_divider: u32,
    /// Secondary reference divider, 1 to 32 (`fref_pll2_div_rs`).
    pub(crate) secondary_divider: u32,
    /// The VCO frequency, in Hz; 0 until a plan uses APLL2 (`vco2_freq`).
    pub(crate) vco_hz: u64,
    /// Feedback divider integer part (`vco2_n`).
    pub(crate) n: u32,
    /// Feedback fraction numerator (`vco2_num`).
    pub(crate) num: u32,
    /// Feedback fraction denominator (`vco2_den`).
    pub(crate) den: u32,
    /// Post-divider 1; 0 when unused (`pd1`).
    pub(crate) pd1: u32,
    /// Post-divider 2; 0 when unused (`pd2`).
    pub(crate) pd2: u32,
}

/// An APLL2 feedback fraction `n + num / den`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Fraction {
    /// Integer part.
    n: u32,
    /// Numerator.
    num: u32,
    /// Denominator, at most 24 bits.
    den: u32,
}

impl Apll2 {
    /// APLL2 as `lmk05318_create` sets it up: reference dividers 3 and the smallest
    /// secondary that keeps the phase detector at or below 150 MHz, nothing planned.
    pub(crate) fn cascaded() -> Self {
        let primary: u32 = 3;
        // The C computes this in 64 bits (`VCO_APLL1` is 64-bit): ceil(2.5e9 / 150e6) is
        // 17, then rounded up over the primary divider: 6.
        let over_pd = VCO1_HZ.div_ceil(u64::from(APLL2_PD_MAX_HZ));
        let secondary = over_pd.div_ceil(u64::from(primary));
        Self {
            primary_divider: primary,
            secondary_divider: c_u32(secondary),
            vco_hz: 0,
            n: 0,
            num: 0,
            den: 0,
            pd1: 0,
            pd2: 0,
        }
    }

    /// The VCO frequency the feedback fraction nearest `vco_hz` gives, and that fraction;
    /// `None` when its reduced denominator passes 24 bits (`lmk05318_calc_vco2_div`, whose
    /// `-EINVAL` the solver can only reject).
    fn reach(&self, vco_hz: u64) -> Option<(f64, Fraction)> {
        let prediv = u64::from(self.primary_divider.wrapping_mul(self.secondary_divider));
        let mut den = VCO1_HZ.wrapping_mul(prediv);
        // `-Ofast`: the division by `VCO_APLL1` is a multiplication by its reciprocal.
        let r = c_f64(vco_hz.wrapping_mul(prediv)) * VCO1_RECIPROCAL;
        let n = c_unsigned(r);
        let fraction = r - f64::from(n);
        let mut num = c_u64(fraction * c_f64(den) + 0.5);
        let divisor = gcd(num, den);
        if divisor > 1 {
            num /= divisor;
            den /= divisor;
        }
        if den > 0xff_ffff {
            return None;
        }
        // Both fit 24 bits now: the numerator stays below the denominator.
        let (num, den) = (c_u32(num), c_u32(den));
        let hz = (f64::from(num) / f64::from(den) + f64::from(n)) * VCO1_F64 / c_f64(prediv);
        Some((hz, Fraction { n, num, den }))
    }

    /// APLL2's settings for the current plan (`lmk05318_tune_apll2`): off when its VCO is
    /// out of range or neither post-divider is valid, otherwise on, with an unused
    /// post-divider set to the other's value (which the state keeps).
    pub(crate) fn settle(&mut self) -> Result<Apll2Setting, ClockPlanError> {
        let fpd2 = VCO1_F64 / f64::from(self.primary_divider) / f64::from(self.secondary_divider);
        if fpd2 < f64::from(APLL2_PD_MIN_HZ) || fpd2 > f64::from(APLL2_PD_MAX_HZ) {
            return Err(ClockPlanError::Apll2PhaseDetector);
        }
        let pd1_valid = APLL2_POST_DIVIDERS.contains(&self.pd1);
        let pd2_valid = APLL2_POST_DIVIDERS.contains(&self.pd2);
        if !(VCO2_MIN_HZ..=VCO2_MAX_HZ).contains(&self.vco_hz) || !pd1_valid && !pd2_valid {
            return Ok(Apll2Setting::Off);
        }
        if !pd1_valid {
            self.pd1 = self.pd2;
        } else if !pd2_valid {
            self.pd2 = self.pd1;
        }
        Ok(Apll2Setting::On {
            n: self.n,
            num: self.num,
            den: self.den,
            pd1: self.pd1,
            pd2: self.pd2,
            order: SdmOrder::for_numerator(self.num.into()),
        })
    }

    /// Plans `requests` (`lmk05318_solver`): each output from APLL1 if it may and APLL1
    /// reaches it exactly, the rest from APLL2, whose VCO and post-dividers this then
    /// chooses; then APLL2's settings. Outputs come back in output order.
    ///
    /// On failure the state may have moved on, as libusdr's does: APLL2's numbers change
    /// once its routing succeeds, even if the final checks then fail.
    pub(crate) fn plan(&mut self, requests: &[OutputRequest]) -> Result<Plan, ClockPlanError> {
        if requests.is_empty() || requests.len() > 8 {
            return Err(ClockPlanError::BadRequest);
        }
        let mut slots = [Slot::default(); SLOTS];
        for request in requests {
            if request.port > 7 {
                return Err(ClockPlanError::BadRequest);
            }
            if request.format.is_lvcmos() && request.port < 4 && request.hz != 0 {
                return Err(ClockPlanError::LvcmosOnLowOutput);
            }
            let slot = &mut slots[slot_of(request.port)];
            let held = &slot.request;
            if held.hz != 0
                && (held.hz != request.hz
                    || held.above_hz != request.above_hz
                    || held.below_hz != request.below_hz
                    || held.inverted != request.inverted
                    || held.affinity != request.affinity)
            {
                return Err(ClockPlanError::ConflictingRequests);
            }
            *slot = Slot::for_request(*request);
        }
        for slot in &mut slots {
            slot.solved = slot.request.hz == 0;
        }
        route_apll1(&mut slots)?;
        let to_solve = slots.iter().filter(|slot| !slot.solved).count();
        if to_solve != 0 {
            limit_post_dividers(&mut slots)?;
            self.route(&mut slots, to_solve, VCO2_MAX_HZ.midpoint(VCO2_MIN_HZ))?;
        }

        let mut sorted = requests.to_vec();
        sorted.sort_by_key(|request| request.port);
        let mut complete = true;
        let outputs: Vec<OutputPlan> = sorted
            .into_iter()
            .map(|request| {
                if request.hz == 0 {
                    return OutputPlan {
                        request,
                        route: None,
                    };
                }
                let route = slots[slot_of(request.port)].route;
                let (low, high) = request.range();
                if route.hz < f64::from(low) || route.hz > f64::from(high) {
                    complete = false;
                }
                OutputPlan {
                    request,
                    route: Some(route),
                }
            })
            .collect();
        if !complete {
            return Err(ClockPlanError::Inexact);
        }
        let apll2 = self.settle()?;
        Ok(Plan { apll2, outputs })
    }

    /// Routes the unsolved slots through APLL2 (`lmk05318_solver_helper`): bands of VCO
    /// frequencies each post-divider and output-divider pair allows, their intersections,
    /// the post-divider bindings those allow, and the first VCO frequency, scanning up
    /// each intersection, at which every bound output divides exactly.
    fn route(
        &mut self,
        slots: &mut [Slot; SLOTS],
        to_solve: usize,
        centre: u64,
    ) -> Result<(), ClockPlanError> {
        let bands = bands(slots, centre);
        if bands.is_empty() {
            return Err(ClockPlanError::OutOfRange);
        }
        let intersections = intersections(slots, &bands, to_solve);
        if intersections.is_empty() {
            return Err(ClockPlanError::NoSolution);
        }
        let solutions: Vec<Solution> = intersections
            .iter()
            .map(|intersection| Solution::gather(intersection, &bands, to_solve))
            .collect();
        if !solutions.iter().any(|solution| solution.valid) {
            return Err(ClockPlanError::NoSolution);
        }
        for solution in solutions.iter().filter(|solution| solution.valid) {
            let Some(binding) = solution.bind(to_solve) else {
                continue;
            };
            for vco_hz in solution.low..=solution.high {
                let Some((hz, fraction)) = self.reach(vco_hz) else {
                    continue;
                };
                if hz < c_f64(solution.low) || hz > c_f64(solution.high) {
                    continue;
                }
                let whole = c_u64(hz);
                if (hz - c_f64(whole)).abs() > 1e-6 {
                    continue;
                }
                if binding.divide(slots, hz) {
                    self.vco_hz = whole;
                    self.n = fraction.n;
                    self.num = fraction.num;
                    self.den = fraction.den;
                    self.pd1 = binding.pd1;
                    self.pd2 = binding.pd2;
                    return Ok(());
                }
            }
        }
        Err(ClockPlanError::NoSolution)
    }
}

/// The solver's working copy of one output slot (an entry of `lmk05318_solver`'s `outs`).
#[derive(Clone, Copy, Debug)]
struct Slot {
    /// The request last placed here; a zero frequency marks an unused slot.
    request: OutputRequest,
    /// Reached, or unused.
    solved: bool,
    /// The largest output divider (`max_odiv`).
    max_divider: u64,
    /// The lowest acceptable frequency, in Hz (`freq_min`).
    low_hz: u32,
    /// The highest acceptable frequency, in Hz (`freq_max`).
    high_hz: u32,
    /// The smallest APLL2 post-divider worth trying (`pd_min`).
    pd_min: u32,
    /// The largest (`pd_max`).
    pd_max: u32,
    /// How it is reached, once solved.
    route: Route,
}

impl Default for Slot {
    fn default() -> Self {
        Self {
            request: OutputRequest::new(0, 0, OutputFormat::Off),
            solved: false,
            max_divider: 0,
            low_hz: 0,
            high_hz: 0,
            pd_min: 0,
            pd_max: 0,
            route: Route::default(),
        }
    }
}

impl Slot {
    /// A slot holding `request`, its range and divider limit set if it is on.
    fn for_request(request: OutputRequest) -> Self {
        let mut slot = Self {
            request,
            ..Self::default()
        };
        if request.hz != 0 {
            (slot.low_hz, slot.high_hz) = request.range();
            slot.max_divider = max_divider(request.port);
        }
        slot
    }

    /// The output divider nearest `input_hz / hz`, and whether it reaches `hz` within 1 µHz
    /// (`lmk05318_get_output_divider`, whose `0` is `true` here).
    fn divider_for(&self, input_hz: f64) -> (u64, bool) {
        let wanted = f64::from(self.request.hz);
        let divider = c_u64(input_hz / wanted + 0.5);
        if divider == 0 || divider > self.max_divider {
            return (divider, false);
        }
        let reached = input_hz / c_f64(divider);
        (divider, (reached - wanted).abs() < 1e-6)
    }
}

/// The slot output `port` uses: outputs 0 and 1 share slot 0, 2 and 3 slot 1.
const fn slot_of(port: u8) -> usize {
    match port {
        0 | 1 => 0,
        2 | 3 => 1,
        port => port as usize - 2,
    }
}

/// An output's largest divider (`lmk05318_max_odiv`): 256, or 2^32 on output 7.
const fn max_divider(port: u8) -> u64 {
    match port {
        0..=6 => 1 << 8,
        7 => 1 << 32,
        _ => 1,
    }
}

/// Routes every unsolved slot APLL1 may drive and reaches exactly; an inverted slot it
/// cannot reach fails the plan.
fn route_apll1(slots: &mut [Slot; SLOTS]) -> Result<(), ClockPlanError> {
    for slot in slots.iter_mut().filter(|slot| !slot.solved) {
        if matches!(slot.request.affinity, Affinity::Any | Affinity::Apll1) {
            let (divider, reached) = slot.divider_for(VCO1_F64);
            if reached {
                slot.solved = true;
                slot.route = Route {
                    mux: if slot.request.inverted {
                        Mux::Apll1Inverted
                    } else {
                        Mux::Apll1
                    },
                    divider,
                    hz: VCO1_F64 / c_f64(divider),
                };
            }
        }
        if !slot.solved && slot.request.inverted {
            return Err(ClockPlanError::InversionNeedsApll1);
        }
    }
    Ok(())
}

/// Sets each unsolved slot's post-divider range from the frequencies its output divider
/// can take in.
fn limit_post_dividers(slots: &mut [Slot; SLOTS]) -> Result<(), ClockPlanError> {
    const LOWEST_INPUT_HZ: u64 = VCO2_MIN_HZ / 7;
    const HIGHEST_INPUT_HZ: u64 = VCO2_MAX_HZ / 2;
    for slot in slots.iter_mut().filter(|slot| !slot.solved) {
        let (low, high) = slot.request.range();
        let lowest = u64::from(low).max(LOWEST_INPUT_HZ);
        let highest = (u64::from(high) * max_divider(slot.request.port)).min(HIGHEST_INPUT_HZ);
        if lowest > highest {
            return Err(ClockPlanError::OutOfRange);
        }
        // The C keeps these as `int`, then 32-bit fields; both are 2 to 7 here.
        slot.pd_min = c_u32(VCO2_MAX_HZ / highest);
        slot.pd_max = c_u32(VCO2_MAX_HZ / lowest);
    }
    Ok(())
}

/// The VCO frequencies one post-divider and output-divider pair allows one slot
/// (`fvco2_range_t`).
#[derive(Clone, Copy, Debug)]
struct Band {
    /// The slot.
    slot: usize,
    /// The post-divider.
    pd: u32,
    /// The output divider.
    divider: u64,
    /// The lowest VCO frequency, in Hz.
    low: u64,
    /// The highest.
    high: u64,
}

/// Each unsolved slot's bands, for every post-divider in its range and the two output
/// dividers nearest what `centre` gives through it.
fn bands(slots: &[Slot; SLOTS], centre: u64) -> Vec<Band> {
    let mut bands = Vec::new();
    for (index, slot) in slots.iter().enumerate().filter(|(_, slot)| !slot.solved) {
        for pd in slot.pd_min..=slot.pd_max {
            let through = c_u64(c_f64(centre) / f64::from(pd) + 0.5);
            let (nearest, _) = slot.divider_for(c_f64(through));
            let next = if nearest < slot.max_divider {
                nearest + 1
            } else {
                0
            };
            for divider in [nearest, next] {
                if divider == 0 || divider > slot.max_divider {
                    continue;
                }
                let scale = u64::from(pd).wrapping_mul(divider);
                let low = scale.wrapping_mul(u64::from(slot.low_hz)).max(VCO2_MIN_HZ);
                let high = scale.wrapping_mul(u64::from(slot.high_hz)).min(VCO2_MAX_HZ);
                if low <= high {
                    bands.push(Band {
                        slot: index,
                        pd,
                        divider,
                        low,
                        high,
                    });
                }
            }
        }
    }
    bands
}

/// A band and the later bands that narrow it (`intersects_t`).
#[derive(Clone, Debug)]
struct Intersection {
    /// The band it starts from.
    primary: usize,
    /// The lowest VCO frequency left, in Hz.
    low: u64,
    /// The highest.
    high: u64,
    /// The later bands that overlapped it, in order.
    others: Vec<usize>,
}

/// For each band, the later bands that overlap it as it narrows; bands of the same slot
/// are skipped unless only one slot is unsolved. Bands nothing overlaps are dropped.
fn intersections(slots: &[Slot; SLOTS], bands: &[Band], to_solve: usize) -> Vec<Intersection> {
    let mut found = Vec::new();
    for (index, primary) in bands.iter().enumerate() {
        let (mut low, mut high) = (primary.low, primary.high);
        let mut others = Vec::new();
        for (later, band) in bands.iter().enumerate().skip(index + 1) {
            let same_output = slots[band.slot].request.port == slots[primary.slot].request.port;
            if same_output && to_solve != 1 {
                continue;
            }
            let (narrow_low, narrow_high) = (low.max(band.low), high.min(band.high));
            if narrow_low > narrow_high {
                continue;
            }
            (low, high) = (narrow_low, narrow_high);
            others.push(later);
        }
        if !others.is_empty() {
            found.push(Intersection {
                primary: index,
                low,
                high,
                others,
            });
        }
    }
    found
}

/// One slot's post-divider and output-divider options in a solution (`solution_var_t`).
#[derive(Clone, Debug)]
struct Variant {
    /// The slot.
    slot: usize,
    /// Its options, at most one per post-divider, in the order met.
    dividers: Vec<(u32, u64)>,
}

/// An intersection regrouped by slot (`solution_t`).
#[derive(Clone, Debug)]
struct Solution {
    /// The lowest VCO frequency, in Hz.
    low: u64,
    /// The highest.
    high: u64,
    /// The slots, the primary band's first.
    variants: Vec<Variant>,
    /// Every unsolved slot appears.
    valid: bool,
}

impl Solution {
    /// Groups `intersection`'s bands by slot, keeping each slot's first output divider per
    /// post-divider.
    fn gather(intersection: &Intersection, bands: &[Band], to_solve: usize) -> Self {
        let primary = &bands[intersection.primary];
        let mut variants = vec![Variant {
            slot: primary.slot,
            dividers: vec![(primary.pd, primary.divider)],
        }];
        for &other in &intersection.others {
            let band = &bands[other];
            let index = if let Some(index) = variants.iter().position(|v| v.slot == band.slot) {
                index
            } else {
                variants.push(Variant {
                    slot: band.slot,
                    dividers: Vec::new(),
                });
                variants.len() - 1
            };
            let variant = &mut variants[index];
            if !variant.dividers.iter().any(|&(pd, _)| pd == band.pd) {
                variant.dividers.push((band.pd, band.divider));
            }
        }
        Self {
            low: intersection.low,
            high: intersection.high,
            valid: variants.len() == to_solve,
            variants,
        }
    }

    /// Binds the slots to at most two post-divider values (the solver's `pd_binds`): the
    /// first slot's first post-divider, every slot offering it, then the rest to one
    /// shared second value. The C tries each of the first leftover slot's post-dividers in
    /// turn and keeps the last outcome, success or failure; so does this.
    fn bind(&self, to_solve: usize) -> Option<Binding> {
        let first = &self.variants[0];
        let (pd1, first_divider) = first.dividers[0];
        let mut post1 = vec![(first.slot, first_divider)];
        let mut leftover = Vec::new();
        for variant in &self.variants[1..] {
            match variant.dividers.iter().find(|&&(pd, _)| pd == pd1) {
                Some(&(_, divider)) => post1.push((variant.slot, divider)),
                None => leftover.push(variant),
            }
        }
        let Some((&head, rest)) = leftover.split_first() else {
            return Some(Binding {
                pd1,
                post1,
                pd2: 0,
                post2: Vec::new(),
            });
        };
        let mut outcome = None;
        for &(pd2, divider) in &head.dividers {
            let mut post2 = vec![(head.slot, divider)];
            for variant in rest {
                match variant.dividers.iter().find(|&&(pd, _)| pd == pd2) {
                    Some(&(_, divider)) => post2.push((variant.slot, divider)),
                    None => break,
                }
            }
            outcome = (post1.len() + post2.len() == to_solve).then(|| Binding {
                pd1,
                post1: post1.clone(),
                pd2,
                post2,
            });
        }
        outcome
    }
}

/// Slots bound to APLL2's two post-dividers.
#[derive(Clone, Debug)]
struct Binding {
    /// Post-divider 1.
    pd1: u32,
    /// The slots it drives (the bound output dividers are recomputed per VCO frequency).
    post1: Vec<(usize, u64)>,
    /// Post-divider 2; 0 when unused.
    pd2: u32,
    /// The slots it drives.
    post2: Vec<(usize, u64)>,
}

impl Binding {
    /// Routes every bound slot from a VCO at `vco_hz`; `false` at the first that no output
    /// divider reaches exactly. Slots routed before that keep their routes, as in the C.
    fn divide(&self, slots: &mut [Slot; SLOTS], vco_hz: f64) -> bool {
        let groups = [(self.pd1, &self.post1), (self.pd2, &self.post2)];
        for (pd, members) in groups {
            for &(index, _) in members {
                let slot = &mut slots[index];
                let input = c_u64(vco_hz / f64::from(pd) + 0.5);
                let (divider, reached) = slot.divider_for(c_f64(input));
                if !reached {
                    return false;
                }
                slot.route = Route {
                    mux: if pd == self.pd1 {
                        Mux::Apll2Post1
                    } else {
                        Mux::Apll2Post2
                    },
                    divider,
                    // `-Ofast` turns `fvco2 / pd / div` into this.
                    hz: vco_hz / (c_f64(divider) * f64::from(pd)),
                };
                slot.solved = true;
            }
        }
        true
    }
}

/// Output 7's two divider stages for a total of `divider`, 1 to 2^32
/// (`lmk05318_set_out_div`): a first stage up to 256 and the second it leaves, rounded.
pub(crate) fn out7_stages(divider: u64) -> (u16, u64) {
    let first = out7_first_stage(divider);
    let second = c_u64(c_f64(divider) / f64::from(first) + 0.5);
    (first, second)
}

/// Output 7's first stage (`lmk05318_factorize_out7div`): the whole divider up to 256,
/// otherwise the largest factor from 256 down to 6 that leaves a second stage within 2^24,
/// or 256 when none does.
fn out7_first_stage(divider: u64) -> u16 {
    if divider <= u64::from(OUT7_STAGE1_MAX) {
        // Fits: at most 256.
        return u16::try_from(divider).expect("invariant: at most 256 here");
    }
    (OUT7_STAGE1_WITH_STAGE2_MIN..=OUT7_STAGE1_MAX)
        .rev()
        .find(|&first| {
            divider % u64::from(first) == 0 && divider / u64::from(first) <= OUT7_STAGE2_MAX
        })
        .unwrap_or(OUT7_STAGE1_MAX)
}

/// Whether output `port` takes `divider` (`lmk05318_set_out_div`'s range check).
pub(crate) const fn divider_fits(port: u8, divider: u64) -> bool {
    port <= 7 && divider >= 1 && divider <= max_divider(port)
}

/// C's `(uint64_t)` conversion of a non-negative double below 2^64; libusdr's values stay
/// within that range.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "C's double-to-integer truncation, on values the C keeps in range"
)]
const fn c_u64(value: f64) -> u64 {
    value as u64
}

/// C's `(unsigned)` conversion of a double as GCC emits it on x86-64: a 64-bit truncation,
/// then the low 32 bits.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "GCC's double-to-unsigned conversion on x86-64"
)]
const fn c_unsigned(value: f64) -> u32 {
    value as i64 as u32
}

/// C's conversion of a 64-bit integer to `double`, rounding to nearest; libusdr's values
/// stay below 2^53, where it is exact.
#[expect(
    clippy::cast_precision_loss,
    reason = "C's integer-to-double conversion, on values below 2^53"
)]
const fn c_f64(value: u64) -> f64 {
    value as f64
}

/// C's implicit conversion of a 64-bit unsigned to `unsigned`: the low 32 bits.
#[expect(
    clippy::cast_possible_truncation,
    reason = "C's implicit narrowing, on values the C keeps within 32 bits"
)]
const fn c_u32(value: u64) -> u32 {
    value as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cascaded_apll2_reference_stays_within_its_phase_detector() {
        let apll2 = Apll2::cascaded();
        assert_eq!((apll2.primary_divider, apll2.secondary_divider), (3, 6));
        let fpd2 = VCO1_F64 / 18.0;
        assert!((10e6..=150e6).contains(&fpd2));
    }

    #[test]
    fn exact_apll1_divisions_leave_apll2_off() {
        let references = [4, 5, 6, 7].map(|port| {
            OutputRequest::new(port, 25_000_000, OutputFormat::LvcmosPN)
                .with_affinity(Affinity::Apll1)
        });
        let plan = Apll2::cascaded().plan(&references).expect("2.5 GHz / 100");
        assert_eq!(plan.apll2, Apll2Setting::Off);
        for output in plan.outputs {
            let route = output.route.expect("on");
            assert_eq!((route.mux, route.divider), (Mux::Apll1, 100));
            assert!((route.hz - 25e6).abs() < 1e-6);
        }
    }

    #[test]
    fn an_apll2_plan_reaches_its_outputs_within_their_tolerance() {
        let mut apll2 = Apll2::cascaded();
        let lo =
            OutputRequest::new(4, 444_000_000, OutputFormat::Lvds).with_affinity(Affinity::Apll2);
        let plan = apll2.plan(&[lo]).expect("444 MHz is reachable");
        let route = plan.outputs[0].route.expect("on");
        assert!(matches!(route.mux, Mux::Apll2Post1 | Mux::Apll2Post2));
        assert!((route.hz - 444e6).abs() <= 2.0);
        assert!((VCO2_MIN_HZ..=VCO2_MAX_HZ).contains(&apll2.vco_hz));
        assert!(matches!(plan.apll2, Apll2Setting::On { .. }));
    }

    #[test]
    fn output_7_splits_dividers_past_256_into_two_stages() {
        assert_eq!(out7_stages(200), (200, 1));
        assert_eq!(out7_stages(1000), (250, 4));
        let (first, second) = out7_stages(1 << 32);
        assert_eq!(u64::from(first) * second, 1 << 32);
        assert!(second <= OUT7_STAGE2_MAX);
    }

    #[test]
    fn lvcmos_on_the_low_outputs_is_refused() {
        let request = OutputRequest::new(1, 25_000_000, OutputFormat::LvcmosPN);
        assert_eq!(
            Apll2::cascaded().plan(&[request]),
            Err(ClockPlanError::LvcmosOnLowOutput)
        );
    }
}
