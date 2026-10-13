//! The two APLLs: power, reference dividers, calibration, sigma-delta modulators,
//! denominator modes, BAW lock detection and VCO status. Their dividers' numbers are plain
//! bytes ([`Reg::Pll1IntegerHigh`] and on).

use bnb::{BitEnum, bitfield, u2, u3, u5, u7};

use super::plan::SdmOrder;
use super::reg::{IndexedLmk05318Register, Lmk05318Register, Reg};

/// One of the two APLLs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Apll {
    /// APLL1, the BAW VCO.
    Apll1,
    /// APLL2, cascaded from APLL1.
    Apll2,
}

/// How APLL1's fraction is held. `SPARE_NVMBASE2_BY2`, 0x27.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Apll1DenominatorMode {
    /// GPIO2 drives LVCMOS rather than open-drain NMOS. `GPIO2_OUT`, bit 1.
    #[bits(1..=1)]
    gpio2_lvcmos: bool,

    /// APLL1 takes a programmed 24-bit numerator and denominator rather than a 40-bit
    /// numerator over a fixed denominator. `APLL1_DEN_MODE`, bit 0.
    #[bits(0..=0)]
    programmed: bool,
}
impl Lmk05318Register for Apll1DenominatorMode {
    const ADDR: Reg = Reg::Apll1DenominatorMode;
}

/// How APLL2's fraction is held, and the DPLL references' coupling.
/// `SPARE_NVMBASE2_BY1`, 0x28.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Apll2DenominatorMode {
    /// SECREF is DC-coupled internally. `SECREF_DC_MODE`, bit 3.
    #[bits(3..=3)]
    secondary_reference_dc: bool,

    /// PRIREF is DC-coupled internally. `PRIREF_DC_MODE`, bit 2.
    #[bits(2..=2)]
    primary_reference_dc: bool,

    /// APLL2 takes a programmed 24-bit denominator rather than a fixed one.
    /// `APLL2_DEN_MODE`, bit 0.
    #[bits(0..=0)]
    programmed: bool,
}
impl Lmk05318Register for Apll2DenominatorMode {
    const ADDR: Reg = Reg::Apll2DenominatorMode;
}

/// The PLLs' clocking. `PLL_CLK_CFG`, 0x2F.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PllClocks {
    /// APLL2's reference is the XO rather than APLL1's VCO (cascaded). `PLL2_RCLK_SEL`,
    /// bit 7.
    #[bits(7..=7)]
    apll2_from_xo: bool,

    /// Which of APLL1's VCO outputs run: bit 0 to the DPLL TDC, the others to APLL1's N
    /// counter, the digital top and APLL2's reference divider (the YAML's description is
    /// cut short). `PLL1_VCO_TO_CNTRS_EN`, bits 2:0.
    #[bits(0..=2)]
    vco1_outputs: u3,
}
impl Lmk05318Register for PllClocks {
    const ADDR: Reg = Reg::PllClocks;
}

/// APLL1's power. `PLL1_CTRL0`, 0x4A.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Pll1Control {
    /// APLL1 powered down; clearing it after a set starts a calibration. `PLL1_PDN`,
    /// bit 0.
    #[bits(0..=0)]
    powered_down: bool,
}
impl Lmk05318Register for Pll1Control {
    const ADDR: Reg = Reg::Pll1Control;
}

/// APLL1's VCO calibration. `PLL1_CALCTRL0`, 0x4F.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Pll1Calibration {
    /// BAW lock detection on. `BAW_LOCKDET_EN`, bit 4.
    #[bits(4..=4)]
    baw_lock_detect: bool,

    /// Closed-loop wait per calibration step (up to seven steps). `PLL1_CLSDWAIT`,
    /// bits 3:2.
    #[bits(2..=3)]
    closed_loop_wait: u2,

    /// Wait before the VCO calibration starts. `PLL1_VCOWAIT`, bits 1:0.
    #[bits(0..=1)]
    vco_wait: u2,
}
impl Lmk05318Register for Pll1Calibration {
    const ADDR: Reg = Reg::Pll1Calibration;
}

/// BAW lock: its status and the high bits of its detection threshold.
/// `BAW_LOCKDET_PPM_MAX_BY1`, 0x50.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct BawLock {
    /// The BAW VCO is locked; libusdr also writes it, 1 without the DPLL. `BAW_LOCK`,
    /// bit 7.
    #[bits(7..=7)]
    locked: bool,

    /// BAW VCO lock detection, high bits. `BAW_LOCK_DET_1`, bits 6:0.
    #[bits(0..=6)]
    detection_high: u7,
}
impl Lmk05318Register for BawLock {
    const ADDR: Reg = Reg::BawLock;
}

/// APLL2's reference dividers and power. `PLL2_CTRL0`, 0x64.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Pll2Control {
    /// The secondary reference divider less one (divides by 1 to 32) in cascaded mode.
    /// `PLL2_RDIV_SEC`, bits 7:3.
    #[bits(3..=7)]
    secondary_divider_minus_one: u5,

    /// The primary reference divider less three (divides by 3 to 6) in cascaded mode.
    /// `PLL2_RDIV_PRE`, bits 2:1.
    #[bits(1..=2)]
    primary_divider_minus_three: u2,

    /// APLL2 powered down; clearing it after a set starts a calibration. `PLL2_PDN`,
    /// bit 0.
    #[bits(0..=0)]
    powered_down: bool,
}
impl Lmk05318Register for Pll2Control {
    const ADDR: Reg = Reg::Pll2Control;
}

impl Pll2Control {
    /// APLL2 with reference dividers `primary` and `secondary`, masked into their fields as
    /// the C's macro masks them.
    pub(super) fn with_dividers(primary: u32, secondary: u32, powered_down: bool) -> Self {
        Self::new()
            .with_secondary_divider_minus_one(u5::from_raw(low_byte(secondary.wrapping_sub(1))))
            .with_primary_divider_minus_three(u2::from_raw(low_byte(primary.wrapping_sub(3))))
            .with_powered_down(powered_down)
    }
}

/// APLL2's post-dividers, each 2 to 7 (code 1 to 6; 0 is invalid); a reset is needed after
/// changing them. `PLL2_CTRL2`, 0x66.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Pll2PostDividers {
    /// Post-divider 2 less one. `PLL2_P2`, bits 6:4.
    #[bits(4..=6)]
    second_minus_one: u3,

    /// Post-divider 1 less one. `PLL2_P1`, bits 2:0.
    #[bits(0..=2)]
    first_minus_one: u3,
}
impl Lmk05318Register for Pll2PostDividers {
    const ADDR: Reg = Reg::Pll2PostDividers;
}

impl Pll2PostDividers {
    /// Post-dividers `pd1` and `pd2`, masked as the C's macro masks them.
    pub(super) fn with_dividers(pd1: u32, pd2: u32) -> Self {
        Self::new()
            .with_second_minus_one(u3::from_raw(low_byte(pd2.wrapping_sub(1))))
            .with_first_minus_one(u3::from_raw(low_byte(pd1.wrapping_sub(1))))
    }
}

/// A sigma-delta modulator's dither (`SDM_DITHER_MODE_*`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2, closed)]
pub(super) enum Dither {
    /// Weak (`SDM_DITHER_MODE_WEAK`); what libusdr uses.
    Weak = 0,
    /// Medium.
    Medium = 1,
    /// Strong.
    Strong = 2,
    /// Off.
    Off = 3,
}

/// A sigma-delta modulator's order (`PLLn_ORDER`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u3)]
#[repr(u8)]
pub(super) enum OrderCode {
    /// Integer mode.
    Integer = 0,
    /// First order.
    First = 1,
    /// Second order.
    Second = 2,
    /// Third order.
    Third = 3,
    /// Fourth order.
    Fourth = 4,
    /// Codes the YAML does not list.
    #[catch_all]
    Other(u3),
}

impl From<SdmOrder> for OrderCode {
    fn from(order: SdmOrder) -> Self {
        match order {
            SdmOrder::Integer => Self::Integer,
            SdmOrder::Third => Self::Third,
        }
    }
}

/// APLL1's sigma-delta modulator. `PLL1_MASHCTRL`, 0x73.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Pll1Modulator {
    /// Dual-phase feedback. `PLL1_DUAL_PH_EN`, bit 7.
    #[bits(7..=7)]
    dual_phase: bool,

    /// The second stage's seed. `PLL1_MASHSEED1`, bit 6.
    #[bits(6..=6)]
    seed_second: bool,

    /// The first stage's seed. `PLL1_MASHSEED0`, bit 5.
    #[bits(5..=5)]
    seed_first: bool,

    /// Dither. `PLL1_DTHRMODE`, bits 4:3.
    #[bits(3..=4)]
    dither: Dither,

    /// Order. `PLL1_ORDER`, bits 2:0.
    #[bits(0..=2)]
    order: OrderCode,
}
impl Lmk05318Register for Pll1Modulator {
    const ADDR: Reg = Reg::Pll1Modulator;
}

/// APLL1's mode. `PLL1_MODE`, 0x74.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Pll1Mode {
    /// Ignore frequency steps from the pins. `PLL1_IGNORE_GPIO_PIN`, bit 2.
    #[bits(2..=2)]
    ignore_pins: bool,

    /// Frequency steps by pin or register. `PLL1_FDEV_EN`, bit 1.
    #[bits(1..=1)]
    frequency_steps: bool,

    /// Driven by the DPLL rather than running free. `PLL1_MODE`, bit 0.
    #[bits(0..=0)]
    dpll: bool,
}
impl Lmk05318Register for Pll1Mode {
    const ADDR: Reg = Reg::Pll1Mode;
}

/// APLL2's sigma-delta modulator. `PLL2_MASHCTRL`, 0x8B.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Pll2Modulator {
    /// Dither. `PLL2_DTHRMODE`, bits 4:3.
    #[bits(3..=4)]
    dither: Dither,

    /// Order. `PLL2_ORDER`, bits 2:0.
    #[bits(0..=2)]
    order: OrderCode,
}
impl Lmk05318Register for Pll2Modulator {
    const ADDR: Reg = Reg::Pll2Modulator;
}

/// An APLL's VCO status. `PLL1_CALSTAT1`, 0x165; `PLL2_CALSTAT1`, 0x16F.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct VcoStatus {
    /// The charge-pump voltage is within its operating range. `PLLn_VM_INSIDE`, bit 5.
    #[bits(5..=5)]
    in_range: bool,
}
impl IndexedLmk05318Register for VcoStatus {
    type Index = Apll;

    fn addr(apll: Apll) -> Reg {
        match apll {
            Apll::Apll1 => Reg::Pll1Status,
            Apll::Apll2 => Reg::Pll2Status,
        }
    }
}

/// The low byte of a C `unsigned`, as its register macros mask it.
const fn low_byte(value: u32) -> u8 {
    value.to_le_bytes()[0]
}
