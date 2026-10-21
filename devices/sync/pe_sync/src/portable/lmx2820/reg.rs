//! The LMX2820's registers as libusdr touches them: addresses and value types.
//!
//! Fields and their meanings are the pinned `hw/lmx2820/lmx2820.yaml`'s (generated into
//! `gen/def_lmx2820.h`). Reserved fields the YAML says to program are kept as each value
//! type's base word, with libusdr's values; where libusdr departs from the YAML, the doc
//! says so.

use bnb::{BitEnum, bitfield, u2, u3, u4, u5, u6, u9, u10, u11, u12, u15};

use crate::portable::ti_spi::Register;

/// Register addresses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Reg {
    /// See [`Control`]. `R0`.
    Control = 0x00,
    /// See [`Sync`]. `R1`.
    Sync = 0x01,
    /// See [`CalibrationClock`]. `R2`.
    CalibrationClock = 0x02,
    /// See [`ReferenceDoubler`]. `R11`.
    ReferenceDoubler = 0x0b,
    /// See [`ReferenceMultiplier`]. `R12`.
    ReferenceMultiplier = 0x0c,
    /// See [`ReferenceDivider`]. `R13`.
    ReferenceDivider = 0x0d,
    /// See [`ReferencePreDivider`]. `R14`.
    ReferencePreDivider = 0x0e,
    /// See [`TemperatureSensor`]. `R19`.
    TemperatureSensor = 0x13,
    /// See [`VcoStart`]. `R22`.
    VcoStart = 0x16,
    /// Reserved; the YAML says to program 0x639, its reset value (`R28_RESERVED_0`).
    /// `PE_SYNC`'s status check reads it back as an SPI probe. `R28`.
    Probe = 0x1c,
    /// See [`ChannelDividers`]. `R32`.
    ChannelDividers = 0x20,
    /// See [`Modulator`]. `R35`.
    Modulator = 0x23,
    /// See [`FeedbackInteger`]. `R36`.
    FeedbackInteger = 0x24,
    /// The fractional denominator's upper 16 bits (`PLL_DEN_U`). `R38`.
    DenominatorHigh = 0x26,
    /// The fractional denominator's lower 16 bits (`PLL_DEN_L`). `R39`.
    DenominatorLow = 0x27,
    /// The fractional numerator's upper 16 bits (`PLL_NUM_U`). `R42`.
    NumeratorHigh = 0x2a,
    /// The fractional numerator's lower 16 bits (`PLL_NUM_L`). `R43`.
    NumeratorLow = 0x2b,
    /// Instant calibration's numerator, `2^32 * NUM / DEN`, upper 16 bits
    /// (`INSTCAL_PLL_NUM_U`). `R44`.
    InstantNumeratorHigh = 0x2c,
    /// Its lower 16 bits (`INSTCAL_PLL_NUM_L`). `R45`.
    InstantNumeratorLow = 0x2d,
    /// See [`Sysref`]. `R64`.
    Sysref = 0x40,
    /// See [`SysrefDivider`]. `R65`.
    SysrefDivider = 0x41,
    /// See [`SysrefDelay`]. `R66`.
    SysrefDelay = 0x42,
    /// See [`SysrefPulses`]. `R67`.
    SysrefPulses = 0x43,
    /// The PSYNC pin's input format and enable. `R68`.
    PhaseSyncInput = 0x44,
    /// See [`SysrefOutput`]. `R69`.
    SysrefOutput = 0x45,
    /// See [`LockStatus`]. `R74`.
    LockStatus = 0x4a,
    /// The VCO calibration's chosen DACISET (`RB_VCO_DACISET`, bits 8:0); read only.
    /// `R75`.
    VcoCurrent = 0x4b,
    /// The temperature sensor code (`RB_TEMP_SENS`, bits 10:0): 0.85 °C per code, less
    /// 415 °C; read only. `R76`.
    Temperature = 0x4c,
    /// See [`OutputA`]. `R78`.
    OutputA = 0x4e,
    /// See [`OutputB`]. `R79`.
    OutputB = 0x4f,
}

impl From<Reg> for u8 {
    fn from(reg: Reg) -> Self {
        reg as Self
    }
}

/// Reset, power-down and VCO calibration. `R0`.
///
/// Base: `R0_RESERVED_0` (bits 15:14) = 1 and `R0_RESERVED_2` (bit 5) = 1, as the YAML says.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Control {
    /// Reserved, programmed 1. `R0_RESERVED_0`, bits 15:14.
    #[bits(14..=15)]
    reserved_high: u2,
    /// Skip amplitude calibration: faster VCO calibration when instant calibration is not
    /// used. `INSTCAL_SKIP_ACAL`, bit 13.
    #[bits(13..=13)]
    skip_amplitude_calibration: bool,
    /// VCO calibration tuned for a high phase-detector frequency. `FCAL_HPFD_ADJ`, bits
    /// 10:9.
    #[bits(9..=10)]
    high_pfd_adjust: HighPfdAdjust,
    /// VCO calibration tuned for a low phase-detector frequency. `FCAL_LPFD_ADJ`, bits 8:7.
    #[bits(7..=8)]
    low_pfd_adjust: LowPfdAdjust,
    /// VCO doubler calibration. `DBLR_CAL_EN`, bit 6.
    #[bits(6..=6)]
    doubler_calibration: bool,
    /// Reserved, programmed 1. `R0_RESERVED_2`, bit 5.
    #[bits(5..=5)]
    reserved_low: bool,
    /// VCO calibration: writing R0 with this set starts it. `FCAL_EN`, bit 4.
    #[bits(4..=4)]
    calibrate: bool,
    /// Every register back to its default; self-clearing. `RESET`, bit 1.
    #[bits(1..=1)]
    reset: bool,
    /// The device powered down. `POWERDOWN`, bit 0.
    #[bits(0..=0)]
    powered_down: bool,
}
impl Register for Control {
    type Map = Reg;
    const ADDR: Reg = Reg::Control;
}

/// `FCAL_HPFD_ADJ`'s settings, by phase-detector frequency.
#[expect(
    clippy::enum_variant_names,
    reason = "the YAML names each setting by its frequency band"
)]
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
pub(crate) enum HighPfdAdjust {
    /// fPD ≤ 100 MHz.
    UpTo100MHz = 0,
    /// 100 MHz < fPD ≤ 150 MHz.
    UpTo150MHz = 1,
    /// 150 MHz < fPD ≤ 200 MHz.
    UpTo200MHz = 2,
    /// fPD > 200 MHz.
    Above200MHz = 3,
}

/// `FCAL_LPFD_ADJ`'s settings, by phase-detector frequency.
#[expect(
    clippy::enum_variant_names,
    reason = "the YAML names each setting by its frequency band"
)]
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
pub(crate) enum LowPfdAdjust {
    /// fPD ≥ 10 MHz.
    From10MHz = 0,
    /// 5 MHz ≤ fPD < 10 MHz.
    From5MHz = 1,
    /// 2.5 MHz ≤ fPD < 5 MHz.
    From2_5MHz = 2,
    /// fPD < 2.5 MHz.
    Below2_5MHz = 3,
}

/// Phase synchronization, lock-detect type and instant calibration. `R1`.
///
/// Base: `R1_RESERVED_0` (bits 14:6) = 0x15E, as the YAML says.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Sync {
    /// Phase synchronization on the PSYNC pin; SYSREF needs it. `PHASE_SYNC_EN`, bit 15.
    #[bits(15..=15)]
    phase_sync: bool,
    /// Reserved, programmed 0x15E. `R1_RESERVED_0`, bits 14:6.
    #[bits(6..=14)]
    reserved: u9,
    /// Lock detect also requires the VCO tuning voltage in range, not only a finished
    /// calibration. `LD_VTUNE_EN`, bit 5.
    #[bits(5..=5)]
    vtune_lock_detect: bool,
    /// The VCO doubler is engaged, for instant calibration. `INSTCAL_DBLR_EN`, bit 1.
    #[bits(1..=1)]
    instant_doubler: bool,
    /// Instant calibration. `INSTCAL_EN`, bit 0.
    #[bits(0..=0)]
    instant_calibration: bool,
}
impl Register for Sync {
    type Map = Reg;
    const ADDR: Reg = Reg::Sync;
}

/// The calibration state machine's clock divider and instant calibration's delay. `R2`.
///
/// Base: `R2_RESERVED_0` (bit 15) = 1, as the YAML says.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CalibrationClock {
    /// Reserved, programmed 1. `R2_RESERVED_0`, bit 15.
    #[bits(15..=15)]
    reserved: bool,
    /// The state machine clock is fOSCIN / 2^this, at most 200 MHz. `CAL_CLK_DIV`, bits
    /// 14:12: 0 for fOSCIN ≤ 200 MHz, 1 ≤ 400 MHz, 2 ≤ 800 MHz, 3 above.
    #[bits(12..=14)]
    divider_log2: u3,
    /// Instant calibration's wait. `INSTCAL_DLY`, bits 11:1; reset 500.
    #[bits(1..=11)]
    instant_delay: u11,
    /// Recalibrate from the current VCO settings. `QUICK_RECAL_EN`, bit 0.
    #[bits(0..=0)]
    quick_recalibration: bool,
}
impl Register for CalibrationClock {
    type Map = Reg;
    const ADDR: Reg = Reg::CalibrationClock;
}

/// The reference input doubler. `R11`.
///
/// Base: `R11_RESERVED_0` (bits 15:5) = 0x30, as the YAML says, and `R11_RESERVED_1`
/// (bits 3:0) = 3, libusdr's choice ("0x3 by doc") where the YAML says 2.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ReferenceDoubler {
    /// Reserved, programmed 0x30. `R11_RESERVED_0`, bits 15:5.
    #[bits(5..=15)]
    reserved_high: u11,
    /// The doubler on. `OSC_2X`, bit 4.
    #[bits(4..=4)]
    doubled: bool,
    /// Reserved: libusdr writes 3. `R11_RESERVED_1`, bits 3:0.
    #[bits(0..=3)]
    reserved_low: u4,
}
impl Register for ReferenceDoubler {
    type Map = Reg;
    const ADDR: Reg = Reg::ReferenceDoubler;
}

/// The reference path's frequency multiplier. `R12`.
///
/// Base: `R12_RESERVED_1` (bits 9:0) = 8, as the YAML says.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ReferenceMultiplier {
    /// 1 bypasses the multiplier; 3 to 7 multiply. `MULT`, bits 12:10.
    #[bits(10..=12)]
    multiplier: u3,
    /// Reserved, programmed 8. `R12_RESERVED_1`, bits 9:0.
    #[bits(0..=9)]
    reserved: u10,
}
impl Register for ReferenceMultiplier {
    type Map = Reg;
    const ADDR: Reg = Reg::ReferenceMultiplier;
}

/// The reference path's post-R divider. `R13`.
///
/// Base: `R13_RESERVED_1` (bits 4:0) = 0x18, as the YAML says.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ReferenceDivider {
    /// The divider, 1 to 255. `PLL_R`, bits 12:5.
    #[bits(5..=12)]
    divider: u8,
    /// Reserved, programmed 0x18. `R13_RESERVED_1`, bits 4:0.
    #[bits(0..=4)]
    reserved: u5,
}
impl Register for ReferenceDivider {
    type Map = Reg;
    const ADDR: Reg = Reg::ReferenceDivider;
}

/// The reference path's pre-R divider. `R14`.
///
/// Base: `R14_RESERVED_0` (bits 15:12) = 3, as the YAML says.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ReferencePreDivider {
    /// Reserved, programmed 3. `R14_RESERVED_0`, bits 15:12.
    #[bits(12..=15)]
    reserved: u4,
    /// The divider, 1 to 4095. `PLL_R_PRE`, bits 11:0.
    #[bits(0..=11)]
    divider: u12,
}
impl Register for ReferencePreDivider {
    type Map = Reg;
    const ADDR: Reg = Reg::ReferencePreDivider;
}

/// The temperature sensor. `R19`.
///
/// Base: `R19_RESERVED_0` (bits 15:5) = 0x109, as the YAML says.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TemperatureSensor {
    /// Reserved, programmed 0x109. `R19_RESERVED_0`, bits 15:5.
    #[bits(5..=15)]
    reserved: u11,
    /// 3 enables the sensor, 0 disables it. `TEMPSENSE_EN`, bits 4:3.
    #[bits(3..=4)]
    enable: u2,
    /// Reserved, programmed 0. `R19_RESERVED_1`, bits 2:0.
    #[bits(0..=2)]
    reserved_low: u3,
}
impl Register for TemperatureSensor {
    type Map = Reg;
    const ADDR: Reg = Reg::TemperatureSensor;
}

/// Where VCO calibration starts: core and sub-band. `R22`.
///
/// Base: `R22_RESERVED_0` (bits 12:8) = 2, as the YAML says.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VcoStart {
    /// The VCO core, 1 to 7. `VCO_SEL`, bits 15:13.
    #[bits(13..=15)]
    core: u3,
    /// Reserved, programmed 2. `R22_RESERVED_0`, bits 12:8.
    #[bits(8..=12)]
    reserved: u5,
    /// The sub-band, 191 (lowest frequency) to 0. `VCO_CAPCTRL`, bits 7:0.
    #[bits(0..=7)]
    sub_band: u8,
}
impl Register for VcoStart {
    type Map = Reg;
    const ADDR: Reg = Reg::VcoStart;
}

/// The two outputs' channel dividers. `R32`.
///
/// Base: `R32_RESERVED_0` (bits 15:12) = 1 and `R32_RESERVED_1` (bits 5:0) = 1, as the
/// YAML says.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ChannelDividers {
    /// Reserved, programmed 1. `R32_RESERVED_0`, bits 15:12.
    #[bits(12..=15)]
    reserved_high: u4,
    /// RFOUTB's divider: 2^(this + 1), 2 to 128. `CHDIVB`, bits 11:9.
    #[bits(9..=11)]
    b_log2_less_one: u3,
    /// RFOUTA's divider, likewise. `CHDIVA`, bits 8:6.
    #[bits(6..=8)]
    a_log2_less_one: u3,
    /// Reserved, programmed 1. `R32_RESERVED_1`, bits 5:0.
    #[bits(0..=5)]
    reserved_low: u6,
}
impl Register for ChannelDividers {
    type Map = Reg;
    const ADDR: Reg = Reg::ChannelDividers;
}

/// The sigma-delta modulator. `R35`.
///
/// Base: `R35_RESERVED_0` (bits 15:13) = 1, as the YAML says.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Modulator {
    /// Reserved, programmed 1. `R35_RESERVED_0`, bits 15:13.
    #[bits(13..=15)]
    reserved: u3,
    /// The modulator out of reset (the active-low `MASH_RESET_N`, bit 12).
    #[bits(12..=12)]
    running: bool,
    /// Its order. `MASH_ORDER`, bits 8:7.
    #[bits(7..=8)]
    order: MashOrder,
    /// MASHSEED phase adjustment. `MASHSEED_EN`, bit 6.
    #[bits(6..=6)]
    seeded: bool,
}
impl Register for Modulator {
    type Map = Reg;
    const ADDR: Reg = Reg::Modulator;
}

/// The modulator's order (`MASH_ORDER`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
pub(crate) enum MashOrder {
    /// Integer division only.
    Integer = 0,
    /// First order.
    First = 1,
    /// Second order.
    Second = 2,
    /// Third order.
    Third = 3,
}

/// The feedback divider's integer part. `R36`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FeedbackInteger {
    /// The integer part, 12 to 32767. `PLL_N`, bits 14:0.
    #[bits(0..=14)]
    n: u15,
}
impl Register for FeedbackInteger {
    type Map = Reg;
    const ADDR: Reg = Reg::FeedbackInteger;
}

/// Lock detect and the VCO calibration's choices; read only. `R74`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LockStatus {
    /// Lock detect. `RB_LD`, bits 15:14.
    #[bits(14..=15)]
    lock: LockDetect,
    /// Not used. `R74_RESERVED_0`, bit 13.
    #[bits(13..=13)]
    reserved: bool,
    /// The sub-band calibration chose. `RB_VCO_CAPCTRL`, bits 12:5.
    #[bits(5..=12)]
    sub_band: u8,
    /// The core calibration chose; 0 is invalid. `RB_VCO_SEL`, bits 4:2.
    #[bits(2..=4)]
    core: u3,
}

/// `RB_LD`'s readings.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
pub(crate) enum LockDetect {
    /// Unlocked (`Unlocked0`).
    Unlocked = 0,
    /// Unlocked (`Unlocked1`).
    UnlockedToo = 1,
    /// Locked.
    Locked = 2,
    /// Invalid.
    Invalid = 3,
}

/// RFOUTA. `R78`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OutputA {
    /// RFOUTA powered down. `OUTA_PD`, bit 4.
    #[bits(4..=4)]
    powered_down: bool,
    /// Its source. `OUTA_MUX`, bits 1:0.
    #[bits(0..=1)]
    source: OutputSource,
}
impl Register for OutputA {
    type Map = Reg;
    const ADDR: Reg = Reg::OutputA;
}

/// RFOUTB, and RFOUTA's power. `R79`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OutputB {
    /// RFOUTB powered down. `OUTB_PD`, bit 8.
    #[bits(8..=8)]
    powered_down: bool,
    /// Its source. `OUTB_MUX`, bits 5:4.
    #[bits(4..=5)]
    source: OutputSource,
    /// RFOUTA's output power; higher is more. `OUTA_PWR`, bits 3:1.
    #[bits(1..=3)]
    a_power: u3,
}
impl Register for OutputB {
    type Map = Reg;
    const ADDR: Reg = Reg::OutputB;
}

/// What an output's mux selects (`OUTA_MUX`, `OUTB_MUX`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2, closed)]
pub(crate) enum OutputSource {
    /// The channel divider.
    ChannelDivider = 0,
    /// The VCO.
    Vco = 1,
    /// The VCO doubler.
    VcoDoubler = 2,
}

/// SYSREF's mode, input format and pre-divider. `R64`: libusdr writes `R64_RESERVED_0`
/// (bits 15:10) 0 with SYSREF on, where the YAML says 0x10 ("0x10 by doc"), and leaves
/// `SYSREF_REPEAT_NS` and `R64_RESERVED_1` at 0.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Sysref {
    /// The SRREQ input's format, or 2 in master mode. `SYSREF_INP_FMT`, bits 9:8.
    #[bits(8..=9)]
    input_format: u2,
    /// The pre-divider: 1 for /2, 2 for /4, 4 for /8. `SYSREF_DIV_PRE`, bits 7:5.
    #[bits(5..=7)]
    pre_divider: u3,
    /// Pulsed rather than continuous. `SYSREF_PULSE`, bit 3.
    #[bits(3..=3)]
    pulsed: bool,
    /// SYSREF on. `SYSREF_EN`, bit 2.
    #[bits(2..=2)]
    enabled: bool,
    /// Repeater rather than master mode. `SYSREF_REPEAT`, bit 1.
    #[bits(1..=1)]
    repeater: bool,
}
impl Register for Sysref {
    type Map = Reg;
    const ADDR: Reg = Reg::Sysref;
}

/// The SYSREF divider. `R65`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SysrefDivider {
    /// The divider less 2 (2 to 2049). `SYSREF_DIV`, bits 10:0.
    #[bits(0..=10)]
    divider_less_two: u11,
}
impl Register for SysrefDivider {
    type Map = Reg;
    const ADDR: Reg = Reg::SysrefDivider;
}

/// SYSREF's delay, DACs 1 and 2. `R66`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SysrefDelay {
    /// `JESD_DAC2_CTRL`, bits 11:6.
    #[bits(6..=11)]
    dac2: u6,
    /// `JESD_DAC1_CTRL`, bits 5:0.
    #[bits(0..=5)]
    dac1: u6,
}
impl Register for SysrefDelay {
    type Map = Reg;
    const ADDR: Reg = Reg::SysrefDelay;
}

/// SYSREF's delay, DACs 3 and 4, and its pulse count. `R67`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SysrefPulses {
    /// Pulses per request in pulsed mode. `SYSREF_PULSE_CNT`, bits 15:12.
    #[bits(12..=15)]
    count: u4,
    /// `JESD_DAC4_CTRL`, bits 11:6.
    #[bits(6..=11)]
    dac4: u6,
    /// `JESD_DAC3_CTRL`, bits 5:0.
    #[bits(0..=5)]
    dac3: u6,
}
impl Register for SysrefPulses {
    type Map = Reg;
    const ADDR: Reg = Reg::SysrefPulses;
}

/// The SYSREF output buffer. `R69`.
///
/// Base: `R69_RESERVED_1` (bits 3:0) = 1, as the YAML says.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SysrefOutput {
    /// SROUT powered down. `SROUT_PD`, bit 4.
    #[bits(4..=4)]
    powered_down: bool,
    /// Reserved, programmed 1. `R69_RESERVED_1`, bits 3:0.
    #[bits(0..=3)]
    reserved: u4,
}
impl Register for SysrefOutput {
    type Map = Reg;
    const ADDR: Reg = Reg::SysrefOutput;
}

/// The SYSREF registers as libusdr programs them with SYSREF off, in its write order
/// (`lmx2820_tune_internal`'s defaults): R69 with the SYSREF
/// output powered down (`SROUT_PD`) and its reserved bits 3:0 at 1; R68 all clear; R67 and
/// R66 with no delay (`JESD_DAC1_CTRL` = 63, the others 0) and no pulses; R65 with
/// `SYSREF_DIV` = 1; R64 with SYSREF disabled, pre-divider 8 (`SYSREF_DIV_PRE` = 4) and its
/// reserved bits 15:10 at 0x10. Each is its YAML reset value.
pub(crate) const SYSREF_OFF: [(Reg, u16); 6] = [
    (Reg::SysrefOutput, 0x0011),
    (Reg::PhaseSyncInput, 0x0000),
    (Reg::SysrefPulses, 0x0000),
    (Reg::SysrefDelay, 0x003f),
    (Reg::SysrefDivider, 0x0001),
    (Reg::Sysref, 0x4080),
];
