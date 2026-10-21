//! The LMX1214's registers as libusdr touches them: addresses and value types.
//!
//! Fields and their meanings are the pinned `hw/lmx1214/lmx1214.yaml`'s (generated into
//! `gen/def_lmx1214.h`). Most of the map is "undisclosed" there, with values to program;
//! registers libusdr writes as constants stay raw words, each value explained.

use bnb::{BitEnum, bitfield, u2, u3, u5, u6, u9, u10, u11, u12};

use crate::portable::ti_spi::Register;

/// Register addresses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Reg {
    /// Power-down (`POWERDOWN`, bit 2) and undisclosed bits programmed 0. `R0`.
    Control = 0x00,
    /// The state machine clock (`SMCLK_EN`, bit 5) and undisclosed bits. `R2`.
    StateMachine = 0x02,
    /// See [`Outputs`]. `R3`.
    Outputs = 0x03,
    /// The auxiliary clock's output power and common mode. `R7`.
    AuxPower = 0x07,
    /// See [`AuxOutput`]. `R8`.
    AuxOutput = 0x08,
    /// See [`AuxDivider`]. `R9`.
    AuxDivider = 0x09,
    /// The SYNC delay step size (`SYNC_DLY_STEP`, bits 1:0): 0 for 28 ps (1.4–2.7 GHz), 1
    /// for 15 ps (2.4–4.7 GHz), 2 for 11 ps (3.1–5.7 GHz), 3 for 8 ps (4.5–12.8 GHz). `R13`.
    SyncDelayStep = 0x0d,
    /// See [`Capture`]. `R14`.
    Capture = 0x0e,
    /// See [`Sync`]. `R15`.
    Sync = 0x0f,
    /// The temperature sensor (`TS_EN`, bit 15) and the MUXOUT pin driver (`MUXOUT_EN`,
    /// bit 13). `R23`.
    TemperatureSensor = 0x17,
    /// See [`TemperatureCount`]. `R24`.
    TemperatureCount = 0x18,
    /// See [`ClockMux`]. `R25`.
    ClockMux = 0x19,
    /// Pin status readbacks, and undisclosed bits 3:0 programmed 6. `R75`.
    PinStatus = 0x4b,
    /// Undisclosed; "Program this field to 0x5", which differs from its reset value. `R79`.
    Undisclosed79 = 0x4f,
    /// `MUXOUT_EN_OVRD` (bit 2): 1 lets `MUXOUT_EN` tri-state the MUXOUT pin. `R86`.
    MuxOutOverride = 0x56,
    /// See [`AuxBypass`]. `R90`.
    AuxBypass = 0x5a,
}

impl From<Reg> for u8 {
    fn from(reg: Reg) -> Self {
        reg as Self
    }
}

/// The four clock outputs. `R3`.
///
/// libusdr programs bits 11:0 to 0xF86 ("0xFE" in its comment and the YAML).
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Outputs {
    /// CLKOUT3 on. `CLKOUT3_EN`, bit 15.
    #[bits(15..=15)]
    out3: bool,
    /// CLKOUT2 on. `CLKOUT2_EN`, bit 14.
    #[bits(14..=14)]
    out2: bool,
    /// CLKOUT1 on. `CLKOUT1_EN`, bit 13.
    #[bits(13..=13)]
    out1: bool,
    /// CLKOUT0 on. `CLKOUT0_EN`, bit 12.
    #[bits(12..=12)]
    out0: bool,
    /// Undisclosed. `R3_UNDISCLOSED`, bits 11:0.
    #[bits(0..=11)]
    undisclosed: u12,
}
impl Register for Outputs {
    type Map = Reg;
    const ADDR: Reg = Reg::Outputs;
}

/// The auxiliary clock output. `R8`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AuxOutput {
    /// The pre-divider: 1, 2 or 4; its output at most 3.2 GHz. `AUXCLK_DIV_PRE`, bits 8:6.
    #[bits(6..=8)]
    pre_divider: u3,
    /// The auxiliary clock subsystem on. `AUXCLKOUT_EN`, bit 4.
    #[bits(4..=4)]
    enabled: bool,
    /// Its driver format. `AUXCLKOUT_FMT`, bits 1:0.
    #[bits(0..=1)]
    format: AuxFormat,
}
impl Register for AuxOutput {
    type Map = Reg;
    const ADDR: Reg = Reg::AuxOutput;
}

/// The auxiliary output's driver format (`AUXCLKOUT_FMT`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2, closed)]
pub(crate) enum AuxFormat {
    /// LVDS.
    Lvds = 0,
    /// CML.
    Cml = 2,
}

/// SYNC latch and clock-position capture. `R14`.
///
/// Base: `R14_UNDISCLOSED_0` (bit 1) = 1, as libusdr writes it.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Capture {
    /// The next SYNC edge latches the clock positions (`RB_CLKPOS`).
    /// `CLKPOS_CAPTURE_EN`, bit 2.
    #[bits(2..=2)]
    enabled: bool,
    /// Undisclosed, programmed 1. `R14_UNDISCLOSED_0`, bit 1.
    #[bits(1..=1)]
    undisclosed: bool,
}
impl Register for Capture {
    type Map = Reg;
    const ADDR: Reg = Reg::Capture;
}

/// The SYNC input's delay. `R15`.
///
/// Base: `R15_UNDISCLOSED_0` (bits 11:7) = 0x16, as libusdr writes it.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Sync {
    /// Undisclosed, programmed 0x16. `R15_UNDISCLOSED_0`, bits 11:7.
    #[bits(7..=11)]
    undisclosed: u5,
    /// SYNC's delay, in [`Reg::SyncDelayStep`] steps. `SYNC_DLY`, bits 6:1.
    #[bits(1..=6)]
    delay: u6,
    /// Holds SYNC cleared while set. `SYNC_CLR`, bit 0.
    #[bits(0..=0)]
    clear: bool,
}
impl Register for Sync {
    type Map = Reg;
    const ADDR: Reg = Reg::Sync;
}

/// The auxiliary clock divider and the SYNC path. `R9`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AuxDivider {
    /// The dividers' synchronization path and clock-position capture. `SYNC_EN`, bit 13.
    #[bits(13..=13)]
    sync: bool,
    /// The divider bypassed, for a total divide of 1 with a pre-divider of 1.
    /// `AUXCLK_DIV_BYP`, bit 11.
    #[bits(11..=11)]
    bypass: bool,
    /// The divider, 2 to 1023. `AUXCLK_DIV`, bits 9:0.
    #[bits(0..=9)]
    divider: u10,
}
impl Register for AuxDivider {
    type Map = Reg;
    const ADDR: Reg = Reg::AuxDivider;
}

/// The auxiliary divider's bypass, second and third steps: both set when
/// [`AuxDivider`]'s bypass is. `R90`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AuxBypass {
    /// `AUXCLK_DIV_BYP3`, bit 6.
    #[bits(6..=6)]
    third: bool,
    /// `AUXCLK_DIV_BYP2`, bit 5.
    #[bits(5..=5)]
    second: bool,
}
impl Register for AuxBypass {
    type Map = Reg;
    const ADDR: Reg = Reg::AuxBypass;
}

/// The temperature sensor's counter and reading. `R24`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TemperatureCount {
    /// The reading: 0.65 °C per code, less 351 °C (`lmx1214_get_temperature`). `RB_TS`,
    /// bits 11:1; read only.
    #[bits(1..=11)]
    code: u11,
    /// The counter on; the sensor must be too. `TS_CNT_EN`, bit 0.
    #[bits(0..=0)]
    counting: bool,
}
impl Register for TemperatureCount {
    type Map = Reg;
    const ADDR: Reg = Reg::TemperatureCount;
}

/// The main clock outputs' function. `R25`.
///
/// Base: `R25_UNDISCLOSED` (bits 15:7) = 4, as the YAML says.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ClockMux {
    /// Undisclosed, programmed 4. `R25_UNDISCLOSED`, bits 15:7.
    #[bits(7..=15)]
    undisclosed: u9,
    /// Resets the main divider while set; toggled after changing it. `CLK_DIV_RST`, bit 6.
    #[bits(6..=6)]
    divider_reset: bool,
    /// The divider less one, 1 to 7 (2 to 8) in divider mode; 0 reverts to buffer mode.
    /// `CLK_DIV`, bits 5:3.
    #[bits(3..=5)]
    divider_less_one: u3,
    /// Buffer or divider mode. `CLK_MUX`, bits 2:0.
    #[bits(0..=2)]
    mode: MainMode,
}
impl Register for ClockMux {
    type Map = Reg;
    const ADDR: Reg = Reg::ClockMux;
}

/// The main outputs' function (`CLK_MUX`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u3, closed)]
pub(crate) enum MainMode {
    /// The input, buffered.
    Buffer = 1,
    /// The input, divided.
    Divider = 2,
}

/// `CLK_DIV_RST`'s bit in [`ClockMux`].
pub(crate) const CLOCK_DIVIDER_RESET: u16 = 1 << 6;

/// What `lmx1214_create` writes, in order: `MUXOUT_EN_OVRD` clear (R86 = 0), R79 = 5 as the
/// YAML says, the sensor's counter on (R24 = 1), then the sensor on with MUXOUT driven
/// push-pull (R23 = 0xE040: `TS_EN`, the undisclosed bit 14 at 1, `MUXOUT_EN`, and
/// undisclosed bit 6, which libusdr's comment calls "MUXOUT=1(SDO)").
pub(crate) const CREATE: [(Reg, u16); 4] = [
    (Reg::MuxOutOverride, 0x0000),
    (Reg::Undisclosed79, 0x0005),
    (Reg::TemperatureCount, 0x0001),
    (Reg::TemperatureSensor, 0xe040),
];

/// `PE_SYNC`'s status check enables SPI readback on MUXOUT this way (R23 = 0x6040: as
/// [`CREATE`]'s R23 with the temperature sensor off).
pub(crate) const MUXOUT_READBACK: (Reg, u16) = (Reg::TemperatureSensor, 0x6040);
