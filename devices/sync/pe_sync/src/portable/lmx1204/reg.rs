//! The LMX1204's registers as libusdr touches them: addresses and value types.
//!
//! Fields and their meanings are the pinned `hw/lmx1204/lmx1204.yaml`'s (generated into
//! `gen/def_lmx1204.h`). Reserved fields keep the values libusdr writes; where those depart
//! from the YAML, the doc says so.

use bnb::{BitEnum, bitfield, u2, u3, u4, u5, u6, u7, u9, u10, u11, u12};

use crate::portable::ti_spi::Register;

/// Register addresses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Reg {
    /// Soft reset (`RESET`, bit 0, self-clearing) and power-down (`POWERDOWN`, bit 2);
    /// writing it with both clear starts a multiplier calibration. `R0`.
    Control = 0x00,
    /// See [`StateMachineClock`]. `R2`.
    StateMachineClock = 0x02,
    /// See [`Channels`]. `R3`.
    Channels = 0x03,
    /// See [`Outputs`]. `R4`.
    Outputs = 0x04,
    /// See [`SysrefDrive`]. `R6`.
    SysrefDrive = 0x06,
    /// See [`LogicSysrefOutput`]. `R7`.
    LogicSysrefOutput = 0x07,
    /// See [`LogicClock`]. `R8`.
    LogicClock = 0x08,
    /// See [`LogicDivider`]. `R9`.
    LogicDivider = 0x09,
    /// The SYSREFREQ delay step size (`SYSREFREQ_DELAY_STEPSIZE`, bits 1:0): 0 for 28 ps
    /// (1.4–2.7 GHz), 1 for 15 ps (2.4–4.7 GHz), 2 for 11 ps (3.1–5.7 GHz), 3 for 8 ps
    /// (4.5–12.8 GHz). `R13`.
    DelayStepSize = 0x0d,
    /// See [`Sysref`]. `R15`.
    Sysref = 0x0f,
    /// See [`SysrefDivider`]. `R16`.
    SysrefDivider = 0x10,
    /// See [`SysrefMode`]. `R17`.
    SysrefMode = 0x11,
    /// See [`SysrefDelay`], SYSREFOUT1 (and SYSREFOUT0's Q). `R18`.
    SysrefDelay1 = 0x12,
    /// SYSREFOUT2 (and SYSREFOUT1's Q). `R19`.
    SysrefDelay2 = 0x13,
    /// SYSREFOUT3 (and SYSREFOUT2's Q). `R20`.
    SysrefDelay3 = 0x14,
    /// LOGISYSREFOUT (and SYSREFOUT3's Q). `R21`.
    LogicSysrefDelay = 0x15,
    /// See [`DelayScale`]. `R22`.
    DelayScale = 0x16,
    /// See [`MuxOut`]. `R23`.
    MuxOut = 0x17,
    /// See [`Temperature`]. `R24`.
    Temperature = 0x18,
    /// See [`ClockMux`]. `R25`.
    ClockMux = 0x19,
    /// Reserved; set before a multiplier calibration (`R33_RESERVED_0`). `R33`.
    MultiplierTrim33 = 0x21,
    /// Reserved; set before a multiplier calibration (`R34_RESERVED_1`). `R34`.
    MultiplierTrim34 = 0x22,
    /// The multiplier VCO core calibration chose (`RB_VCO_SEL`, bits 8:4); read only.
    /// `R65`.
    VcoSelect = 0x41,
    /// Reserved; set before a multiplier calibration (`R67_RESERVED_0`). `R67`.
    MultiplierTrim67 = 0x43,
    /// SYSREF delay generator retiming (`SYSREF_DELAY_BYPASS`, bits 1:0), the SYSREFREQ
    /// SPI trigger and the pulser latch. `R72`.
    SysrefRetime = 0x48,
    /// See [`LockStatus`]. `R75`.
    LockStatus = 0x4b,
    /// Not in the pinned YAML; libusdr writes it 0 ("R76==0"). `R76`.
    Undocumented76 = 0x4c,
    /// Reserved; 0x104 right after the logic divider's bypass is set, 5 otherwise
    /// (libusdr; the YAML says the register can be skipped then). `R79`.
    LogicBypass79 = 0x4f,
    /// Reserved; 0x60 right after R79 when the logic divider is bypassed, else 0. `R90`.
    LogicBypass90 = 0x5a,
    /// Reserved; 4 lets `MUXOUT_EN` tri-state the MUXOUT pin after SPI readback. `R86`.
    MuxOutOverride = 0x56,
}

impl From<Reg> for u8 {
    fn from(reg: Reg) -> Self {
        reg as Self
    }
}

/// The state machine clock, which calibrates the multiplier and runs its lock detect.
/// `R2`.
///
/// Base: `R2_RESERVED_2` (bits 4:0) = 3, as the YAML says.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StateMachineClock {
    /// Its pre-divider from CLKIN: 2, 4 or 8, its output at most 1.6 GHz.
    /// `SMCLK_DIV_PRE`, bits 9:6.
    #[bits(6..=9)]
    pre_divider: u4,
    /// The clock on. `SMCLK_EN`, bit 5.
    #[bits(5..=5)]
    enabled: bool,
    /// Reserved, programmed 3. `R2_RESERVED_2`, bits 4:0.
    #[bits(0..=4)]
    reserved: u5,
}
impl Register for StateMachineClock {
    type Map = Reg;
    const ADDR: Reg = Reg::StateMachineClock;
}

/// The channels and the state machine clock's divider. `R3`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Channels {
    /// CH3 (CLKOUT3, SYSREFOUT3) on. `CH3_EN`, bit 15.
    #[bits(15..=15)]
    ch3: bool,
    /// CH2 on. `CH2_EN`, bit 14.
    #[bits(14..=14)]
    ch2: bool,
    /// CH1 on. `CH1_EN`, bit 13.
    #[bits(13..=13)]
    ch1: bool,
    /// CH0 on. `CH0_EN`, bit 12.
    #[bits(12..=12)]
    ch0: bool,
    /// Every channel and the logic outputs muted during multiplier calibration
    /// (`LOGIC_MUTE_CAL`, `CH3_MUTE_CAL` .. `CH0_MUTE_CAL`, bits 11:7).
    #[bits(7..=11)]
    mute_during_calibration: u5,
    /// The state machine clock's divider, 2^this, its output at most 30 MHz. `SMCLK_DIV`,
    /// bits 2:0.
    #[bits(0..=2)]
    state_machine_divider_log2: u3,
}
impl Register for Channels {
    type Map = Reg;
    const ADDR: Reg = Reg::Channels;
}

/// The four channels' output buffers. `R4`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Outputs {
    /// CLKOUT1's power; larger is more. `CLKOUT1_PWR`, bits 13:11.
    #[bits(11..=13)]
    clock1_power: u3,
    /// CLKOUT0's power. `CLKOUT0_PWR`, bits 10:8.
    #[bits(8..=10)]
    clock0_power: u3,
    /// SYSREFOUT3 to SYSREFOUT0 on, bit n + 4 for output n (`SYSREFOUTn_EN`).
    #[bits(4..=7)]
    sysref: u4,
    /// CLKOUT3 to CLKOUT0 on, bit n for output n (`CLKOUTn_EN`).
    #[bits(0..=3)]
    clock: u4,
}
impl Register for Outputs {
    type Map = Reg;
    const ADDR: Reg = Reg::Outputs;
}

/// SYSREF output drive and the logic clock output. `R6`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SysrefDrive {
    /// LOGICLKOUT's buffer on. `LOGICLKOUT_EN`, bit 15.
    #[bits(15..=15)]
    logic_clock: bool,
    /// SYSREFOUT3 to SYSREFOUT0's output common modes, three bits each from bit 12 down
    /// (`SYSREFOUTn_VCM`).
    #[bits(3..=14)]
    common_modes: u12,
    /// SYSREFOUT3's power. `SYSREFOUT3_PWR`, bits 2:0.
    #[bits(0..=2)]
    sysref3_power: u3,
}
impl Register for SysrefDrive {
    type Map = Reg;
    const ADDR: Reg = Reg::SysrefDrive;
}

/// The logic SYSREF output. `R7`: libusdr leaves its power, pre-driver and common-mode
/// fields at 0.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LogicSysrefOutput {
    /// LOGISYSREFOUT's buffer on. `LOGISYSREFOUT_EN`, bit 0.
    #[bits(0..=0)]
    enabled: bool,
}
impl Register for LogicSysrefOutput {
    type Map = Reg;
    const ADDR: Reg = Reg::LogicSysrefOutput;
}

/// The logic clock subsystem: pre-divider and output formats. `R8`.
///
/// Base: `R8_RESERVED_1` (bit 5) = 1, as the YAML says.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LogicClock {
    /// The pre-divider: 1, 2 or 4, its output at most 3.2 GHz. `LOGICLK_DIV_PRE`, bits
    /// 8:6.
    #[bits(6..=8)]
    pre_divider: u3,
    /// Reserved, programmed 1. `R8_RESERVED_1`, bit 5.
    #[bits(5..=5)]
    reserved: bool,
    /// The logic subsystem on. `LOGIC_EN`, bit 4.
    #[bits(4..=4)]
    enabled: bool,
    /// LOGISYSREFOUT's format. `LOGISYSREFOUT_FMT`, bits 3:2.
    #[bits(2..=3)]
    sysref_format: Format,
    /// LOGICLKOUT's format. `LOGICLKOUT_FMT`, bits 1:0.
    #[bits(0..=1)]
    clock_format: Format,
}
impl Register for LogicClock {
    type Map = Reg;
    const ADDR: Reg = Reg::LogicClock;
}

/// A logic output's driver format (`LOGICLKOUT_FMT`, `LOGISYSREFOUT_FMT`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2, closed)]
pub(crate) enum Format {
    /// LVDS.
    Lvds = 0,
    /// LVPECL.
    Lvpecl = 1,
    /// CML.
    Cml = 2,
}

/// The logic clock divider. `R9`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LogicDivider {
    /// The dividers' synchronization path. `SYNC_EN`, bit 13.
    #[bits(13..=13)]
    sync: bool,
    /// The divider bypassed, for divide-by-1 with a pre-divider of 1.
    /// `LOGICLK_DIV_BYPASS`, bit 11.
    #[bits(11..=11)]
    bypass: bool,
    /// The divider, 2 to 1023. `LOGICLK_DIV`, bits 9:0.
    #[bits(0..=9)]
    divider: u10,
}
impl Register for LogicDivider {
    type Map = Reg;
    const ADDR: Reg = Reg::LogicDivider;
}

/// The SYSREF subsystem. `R15`.
///
/// libusdr programs `R15_RESERVED_1` (bits 9:8) to 3, where the YAML says 1.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Sysref {
    /// The SYSREF pre-divider, 2^this (0 to 2), its output at most 3.2 GHz.
    /// `SYSREF_DIV_PRE`, bits 11:10.
    #[bits(10..=11)]
    pre_divider_log2: u2,
    /// Reserved: libusdr writes 3. `R15_RESERVED_1`, bits 9:8.
    #[bits(8..=9)]
    reserved: u2,
    /// The SYSREF (and SYNC) subsystem on. `SYSREF_EN`, bit 7.
    #[bits(7..=7)]
    enabled: bool,
    /// The SYSREFREQ delay line step. `SYSREFREQ_DELAY_STEP`, bits 6:1.
    #[bits(1..=6)]
    request_delay: u6,
    /// Holds SYSREFREQ cleared while set. `SYSREFREQ_CLR`, bit 0.
    #[bits(0..=0)]
    clear: bool,
}
impl Register for Sysref {
    type Map = Reg;
    const ADDR: Reg = Reg::Sysref;
}

/// The SYSREF divider and pulse count. `R16`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SysrefDivider {
    /// Pulses per request in pulser mode. `SYSREF_PULSE_COUNT`, bits 15:12.
    #[bits(12..=15)]
    pulses: u4,
    /// The divider, 2 to 4095. `SYSREF_DIV`, bits 11:0.
    #[bits(0..=11)]
    divider: u12,
}
impl Register for SysrefDivider {
    type Map = Reg;
    const ADDR: Reg = Reg::SysrefDivider;
}

/// The SYSREF mode and SYSREFOUT0's delay. `R17`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SysrefMode {
    /// SYSREFOUT0's delay step; with its Q, sums to 0x7F. `SYSREFOUT0_DELAY_I`, bits 10:4.
    #[bits(4..=10)]
    delay_i: u7,
    /// SYSREFOUT0's interpolator phase. `SYSREFOUT0_DELAY_PHASE`, bits 3:2.
    #[bits(2..=3)]
    delay_phase: u2,
    /// How SYSREF is made. `SYSREF_MODE`, bits 1:0.
    #[bits(0..=1)]
    mode: SysrefModeCode,
}
impl Register for SysrefMode {
    type Map = Reg;
    const ADDR: Reg = Reg::SysrefMode;
}

/// `SYSREF_MODE`'s settings.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2, closed)]
pub(crate) enum SysrefModeCode {
    /// Continuous (generator mode).
    Continuous = 0,
    /// Pulser (generator mode).
    Pulser = 1,
    /// Repeater.
    Repeater = 2,
}

/// One SYSREF output's delay (I and phase) with the previous output's Q: R18 to R21 share
/// this layout (`SYSREFOUTn_DELAY_I`, `SYSREFOUTn_DELAY_PHASE`, `SYSREFOUTn-1_DELAY_Q`).
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SysrefDelay {
    /// This output's delay step, bits 15:9.
    #[bits(9..=15)]
    delay_i: u7,
    /// This output's interpolator phase, bits 8:7.
    #[bits(7..=8)]
    delay_phase: u2,
    /// The previous output's Q, bits 6:0.
    #[bits(0..=6)]
    previous_q: u7,
}

/// The delay generators' clock division and scales. `R22`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DelayScale {
    /// SYSREFOUT1's delay scale. `SYSREFOUT1_DELAY_SCALE`, bits 15:14.
    #[bits(14..=15)]
    scale1: u2,
    /// SYSREFOUT0's delay scale. `SYSREFOUT0_DELAY_SCALE`, bits 13:12.
    #[bits(12..=13)]
    scale0: u2,
    /// The delay generator clock's division from CLKIN. `SYSREF_DELAY_DIV`, bits 11:9.
    #[bits(9..=11)]
    delay_divider: u3,
    /// LOGISYSREFOUT's delay Q. `LOGISYSREFOUT_DELAY_Q`, bits 6:0.
    #[bits(0..=6)]
    logic_q: u7,
}
impl Register for DelayScale {
    type Map = Reg;
    const ADDR: Reg = Reg::DelayScale;
}

/// The temperature sensor, the MUXOUT pin and the remaining delay scales. `R23`.
///
/// Base: `R23_RESERVED_0` (bit 14) = 1, as the YAML says.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MuxOut {
    /// The temperature sensor on. `EN_TEMPSENSE`, bit 15.
    #[bits(15..=15)]
    temperature_sensor: bool,
    /// Reserved, programmed 1. `R23_RESERVED_0`, bit 14.
    #[bits(14..=14)]
    reserved: bool,
    /// The MUXOUT pin driven push-pull rather than tri-stated. `MUXOUT_EN`, bit 13.
    #[bits(13..=13)]
    driven: bool,
    /// MUXOUT carries SPI readback (SDO) rather than lock detect. `MUXOUT_SEL`, bit 6.
    #[bits(6..=6)]
    sdo: bool,
    /// LOGISYSREFOUT's delay scale. `LOGISYSREFOUT_DELAY_SCALE`, bits 5:4.
    #[bits(4..=5)]
    logic_scale: u2,
    /// SYSREFOUT3's delay scale. `SYSREFOUT3_DELAY_SCALE`, bits 3:2.
    #[bits(2..=3)]
    scale3: u2,
    /// SYSREFOUT2's delay scale. `SYSREFOUT2_DELAY_SCALE`, bits 1:0.
    #[bits(0..=1)]
    scale2: u2,
}
impl Register for MuxOut {
    type Map = Reg;
    const ADDR: Reg = Reg::MuxOut;
}

/// The temperature sensor's counter and reading. `R24`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Temperature {
    /// The reading: 0.65 °C per code, less 351 °C. `RB_TEMPSENSE`, bits 11:1; read only.
    #[bits(1..=11)]
    code: u11,
    /// The counter on; the sensor must be too. `EN_TS_COUNT`, bit 0.
    #[bits(0..=0)]
    counting: bool,
}
impl Register for Temperature {
    type Map = Reg;
    const ADDR: Reg = Reg::Temperature;
}

/// The device's function: buffer, divider or multiplier. `R25`.
///
/// Base: `R25_RESERVED_0` (bits 15:7) = 4, as the YAML says.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ClockMux {
    /// Reserved, programmed 4. `R25_RESERVED_0`, bits 15:7.
    #[bits(7..=15)]
    reserved: u9,
    /// Resets the main divider while set. `CLK_DIV_RST`, bit 6.
    #[bits(6..=6)]
    divider_reset: bool,
    /// The divider less one (1 to 7) in divider mode, the multiplier (1 to 4) in
    /// multiplier mode. `CLK_DIVCLK_MULT`, bits 5:3.
    #[bits(3..=5)]
    ratio: u3,
    /// The function. `CLK_MUX`, bits 2:0.
    #[bits(0..=2)]
    mode: Mode,
}
impl Register for ClockMux {
    type Map = Reg;
    const ADDR: Reg = Reg::ClockMux;
}

/// The device's function (`CLK_MUX`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u3, closed)]
pub(crate) enum Mode {
    /// CLKIN buffered.
    Buffer = 1,
    /// CLKIN divided.
    Divider = 2,
    /// CLKIN multiplied.
    Multiplier = 3,
}

/// The multiplier PLL's lock detect; read only. `R75`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LockStatus {
    /// 2 is locked, 0 and 3 unlocked (VTUNE low, high); meaningless outside multiplier
    /// mode. `RB_LD`, bits 9:8.
    #[bits(8..=9)]
    lock: u2,
}

impl LockStatus {
    /// `RB_LD_LOCKED`.
    pub(crate) const fn locked(self) -> bool {
        self.lock().value() == 2
    }
}

impl MuxOut {
    /// MUXOUT driven with SPI readback, delay scales 0, and the temperature sensor on or
    /// off: `lmx1204_create` writes it with the sensor (0xE040), `PE_SYNC`'s status check
    /// without (0x6040).
    pub(crate) const fn readback(temperature_sensor: bool) -> Self {
        Self::new()
            .with_temperature_sensor(temperature_sensor)
            .with_reserved(true)
            .with_driven(true)
            .with_sdo(true)
    }
}
