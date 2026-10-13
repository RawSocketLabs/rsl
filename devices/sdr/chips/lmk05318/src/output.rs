//! The eight outputs: each one's source mux, driver format and divider, their
//! synchronization, and muting.
//!
//! Outputs 0 and 1 share a mux and a divider (on `OUTCTL_0` and `OUTDIV_0_1`), as do 2 and
//! 3; outputs 1 and 3 have their own format only. Output 7's divider has a second stage.

use bnb::{BitEnum, bitfield, u2, u6};

use super::plan::{Mux, OutputFormat};
use super::reg::{IndexedLmk05318Register, Lmk05318Register, Reg};

/// An output with its own mux: 0 (for 0 and 1), 2 (for 2 and 3), 4, 5, 6 or 7.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MuxedOutput {
    /// Outputs 0 and 1.
    Out0,
    /// Outputs 2 and 3.
    Out2,
    /// Output 4.
    Out4,
    /// Output 5.
    Out5,
    /// Output 6.
    Out6,
    /// Output 7.
    Out7,
}

/// An output whose mux and divider are its pair's: 1 or 3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairedOutput {
    /// Output 1, paired with 0.
    Out1,
    /// Output 3, paired with 2.
    Out3,
}

/// An output divider: 0 and 1's, 2 and 3's, or output 4, 5, 6 or 7's (7's first stage).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DividedOutput {
    /// Outputs 0 and 1.
    Out01,
    /// Outputs 2 and 3.
    Out23,
    /// Output 4.
    Out4,
    /// Output 5.
    Out5,
    /// Output 6.
    Out6,
    /// Output 7, first stage.
    Out7,
}

/// Where output `port`'s control sits: its own mux register or, for 1 and 3, a format-only
/// one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputControlReg {
    /// A mux and format register.
    Muxed(MuxedOutput),
    /// A format-only register.
    Paired(PairedOutput),
}

impl OutputControlReg {
    /// The control register of output `port`, 0 to 7.
    #[must_use]
    pub const fn of(port: u8) -> Option<Self> {
        Some(match port {
            0 => Self::Muxed(MuxedOutput::Out0),
            1 => Self::Paired(PairedOutput::Out1),
            2 => Self::Muxed(MuxedOutput::Out2),
            3 => Self::Paired(PairedOutput::Out3),
            4 => Self::Muxed(MuxedOutput::Out4),
            5 => Self::Muxed(MuxedOutput::Out5),
            6 => Self::Muxed(MuxedOutput::Out6),
            7 => Self::Muxed(MuxedOutput::Out7),
            _ => return None,
        })
    }

    /// Its address.
    #[must_use]
    pub fn addr(self) -> Reg {
        match self {
            Self::Muxed(output) => OutputControl::addr(output),
            Self::Paired(output) => OutputFormatOnly::addr(output),
        }
    }
}

impl DividedOutput {
    /// The divider of output `port`, 0 to 7.
    pub(crate) const fn of(port: u8) -> Option<Self> {
        Some(match port {
            0 | 1 => Self::Out01,
            2 | 3 => Self::Out23,
            4 => Self::Out4,
            5 => Self::Out5,
            6 => Self::Out6,
            7 => Self::Out7,
            _ => return None,
        })
    }
}

/// The clock an output's mux selects (`CHn_MUX`; the YAML's `x-out-pll-sel`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2, closed)]
pub enum MuxCode {
    /// APLL1 (`APLL1_P1`).
    Apll1 = 0,
    /// APLL1, inverted (`APLL1 P1_INV`).
    Apll1Inverted = 1,
    /// APLL2 through post-divider 1 (`APLL2_P1`).
    Apll2Post1 = 2,
    /// APLL2 through post-divider 2 (`APLL2_P2`).
    Apll2Post2 = 3,
}

impl From<Mux> for MuxCode {
    fn from(mux: Mux) -> Self {
        match mux {
            Mux::Apll1 => Self::Apll1,
            Mux::Apll1Inverted => Self::Apll1Inverted,
            Mux::Apll2Post1 => Self::Apll2Post1,
            Mux::Apll2Post2 => Self::Apll2Post2,
        }
    }
}

/// An output's driver format (`OUTn_FMT`; the YAML's `x-out-opts`). The LVCMOS formats
/// name the P pin, then the N pin.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u6)]
#[repr(u8)]
pub enum FormatCode {
    /// Disabled.
    Disabled = 0x00,
    /// AC-coupled LVDS.
    Lvds = 0x10,
    /// AC-coupled CML.
    Cml = 0x14,
    /// AC-coupled LVPECL.
    Lvpecl = 0x18,
    /// HCSL, external 50 Ω termination.
    HcslExternal50 = 0x2c,
    /// HCSL, internal 50 Ω termination.
    HcslInternal50 = 0x2d,
    /// LVCMOS, high-impedance, high-impedance.
    LvcmosHizHiz = 0x30,
    /// LVCMOS, high-impedance, inverted.
    LvcmosHizN = 0x32,
    /// LVCMOS, high-impedance, in phase.
    LvcmosHizP = 0x33,
    /// LVCMOS, low, low.
    LvcmosLowLow = 0x35,
    /// LVCMOS, inverted, high-impedance.
    LvcmosNHiz = 0x38,
    /// LVCMOS, inverted, inverted.
    LvcmosNN = 0x3a,
    /// LVCMOS, inverted, in phase.
    LvcmosNP = 0x3b,
    /// LVCMOS, in phase, high-impedance.
    LvcmosPHiz = 0x3c,
    /// LVCMOS, in phase, inverted.
    LvcmosPN = 0x3e,
    /// LVCMOS, in phase, in phase.
    LvcmosPP = 0x3f,
    /// Reserved codes.
    #[catch_all]
    Other(u6),
}

impl From<OutputFormat> for FormatCode {
    /// libusdr's `lmk05318_decode_fmt`; off is disabled.
    fn from(format: OutputFormat) -> Self {
        match format {
            OutputFormat::Off => Self::Disabled,
            OutputFormat::Lvds => Self::Lvds,
            OutputFormat::Cml => Self::Cml,
            OutputFormat::Lvpecl => Self::Lvpecl,
            OutputFormat::HcslExternal50 => Self::HcslExternal50,
            OutputFormat::HcslInternal50 => Self::HcslInternal50,
            OutputFormat::LvcmosHizHiz => Self::LvcmosHizHiz,
            OutputFormat::LvcmosHizN => Self::LvcmosHizN,
            OutputFormat::LvcmosHizP => Self::LvcmosHizP,
            OutputFormat::LvcmosLowLow => Self::LvcmosLowLow,
            OutputFormat::LvcmosNHiz => Self::LvcmosNHiz,
            OutputFormat::LvcmosNN => Self::LvcmosNN,
            OutputFormat::LvcmosNP => Self::LvcmosNP,
            OutputFormat::LvcmosPHiz => Self::LvcmosPHiz,
            OutputFormat::LvcmosPN => Self::LvcmosPN,
            OutputFormat::LvcmosPP => Self::LvcmosPP,
        }
    }
}

/// An output's source and format. `OUTCTL_0`, 0x33 (outputs 0 and 1's mux, output 0's
/// format); `OUTCTL_2`, 0x36; `OUTCTL_4`-`OUTCTL_7`, 0x39, 0x3B, 0x3D, 0x3F.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputControl {
    /// The source. `CHn_MUX`, bits 7:6.
    #[bits(6..=7)]
    mux: MuxCode,

    /// The driver format. `OUTn_FMT`, bits 5:0.
    #[bits(0..=5)]
    format: FormatCode,
}
impl IndexedLmk05318Register for OutputControl {
    type Index = MuxedOutput;

    fn addr(output: MuxedOutput) -> Reg {
        match output {
            MuxedOutput::Out0 => Reg::Output0Control,
            MuxedOutput::Out2 => Reg::Output2Control,
            MuxedOutput::Out4 => Reg::Output4Control,
            MuxedOutput::Out5 => Reg::Output5Control,
            MuxedOutput::Out6 => Reg::Output6Control,
            MuxedOutput::Out7 => Reg::Output7Control,
        }
    }
}

/// Output 1's or 3's format; bits 7:6 are undocumented, and a read-modify-write keeps them.
/// `OUTCTL_1`, 0x34; `OUTCTL_3`, 0x37.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputFormatOnly {
    /// The driver format. `OUT1_FMT`/`OUT3_FMT`, bits 5:0.
    #[bits(0..=5)]
    format: FormatCode,
}
impl IndexedLmk05318Register for OutputFormatOnly {
    type Index = PairedOutput;

    fn addr(output: PairedOutput) -> Reg {
        match output {
            PairedOutput::Out1 => Reg::Output1Control,
            PairedOutput::Out3 => Reg::Output3Control,
        }
    }
}

/// An output divider: the ratio less one, 1 to 256. `OUTDIV_0_1`, 0x35; `OUTDIV_2_3`,
/// 0x38; `OUTDIV_4`-`OUTDIV_6`, 0x3A, 0x3C, 0x3E; `OUTDIV_7` (the first stage), 0x43.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputDivider {
    /// The ratio less one. `OUTn_DIV`, bits 7:0.
    #[bits(0..=7)]
    ratio_minus_one: u8,
}
impl IndexedLmk05318Register for OutputDivider {
    type Index = DividedOutput;

    fn addr(output: DividedOutput) -> Reg {
        match output {
            DividedOutput::Out01 => Reg::Output01Divider,
            DividedOutput::Out23 => Reg::Output23Divider,
            DividedOutput::Out4 => Reg::Output4Divider,
            DividedOutput::Out5 => Reg::Output5Divider,
            DividedOutput::Out6 => Reg::Output6Divider,
            DividedOutput::Out7 => Reg::Output7Divider,
        }
    }
}

impl OutputDivider {
    /// The register for a divide ratio, truncated to eight bits as the C's macros do.
    pub(crate) fn ratio(ratio: u64) -> Self {
        Self::new().with_ratio_minus_one(ratio.wrapping_sub(1).to_le_bytes()[0])
    }
}

/// Divider synchronization per post-divider. `OUTSYNCCTL`, 0x46.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PostDividerSync {
    /// APLL2 post-divider 2's channels. `PLL2_P2_SYNC_EN`, bit 2.
    #[bits(2..=2)]
    apll2_post2: bool,

    /// APLL2 post-divider 1's channels. `PLL2_P1_SYNC_EN`, bit 1.
    #[bits(1..=1)]
    apll2_post1: bool,

    /// APLL1's channels. `PLL1_P1_SYNC_EN`, bit 0.
    #[bits(0..=0)]
    apll1: bool,
}
impl Lmk05318Register for PostDividerSync {
    const ADDR: Reg = Reg::PostDividerSync;
}

/// Output synchronization per channel. `OUTSYNCEN`, 0x47.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputSync {
    /// Output 7. `CH7_SYNC_EN`, bit 5.
    #[bits(5..=5)]
    output7: bool,

    /// Output 6. `CH6_SYNC_EN`, bit 4.
    #[bits(4..=4)]
    output6: bool,

    /// Output 5. `CH5_SYNC_EN`, bit 3.
    #[bits(3..=3)]
    output5: bool,

    /// Output 4. `CH4_SYNC_EN`, bit 2.
    #[bits(2..=2)]
    output4: bool,

    /// Outputs 2 and 3. `CH2_3_SYNC_EN`, bit 1.
    #[bits(1..=1)]
    outputs23: bool,

    /// Outputs 0 and 1. `CH0_1_SYNC_EN`, bit 0.
    #[bits(0..=0)]
    outputs01: bool,
}
impl Lmk05318Register for OutputSync {
    const ADDR: Reg = Reg::OutputSync;
}

/// Muted outputs, one bit each, output 7 in bit 7. `OUT_MUTE`, 0x19.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputMute {
    /// Output 7. `CH7_MUTE`, bit 7.
    #[bits(7..=7)]
    output7: bool,

    /// Output 6. `CH6_MUTE`, bit 6.
    #[bits(6..=6)]
    output6: bool,

    /// Output 5. `CH5_MUTE`, bit 5.
    #[bits(5..=5)]
    output5: bool,

    /// Output 4. `CH4_MUTE`, bit 4.
    #[bits(4..=4)]
    output4: bool,

    /// Output 3. `CH3_MUTE`, bit 3.
    #[bits(3..=3)]
    output3: bool,

    /// Output 2. `CH2_MUTE`, bit 2.
    #[bits(2..=2)]
    output2: bool,

    /// Output 1. `CH1_MUTE`, bit 1.
    #[bits(1..=1)]
    output1: bool,

    /// Output 0. `CH0_MUTE`, bit 0.
    #[bits(0..=0)]
    output0: bool,
}
impl Lmk05318Register for OutputMute {
    const ADDR: Reg = Reg::OutputMute;
}

/// Outputs muted while a PLL locks. `DPLL_MUTE`, 0x1D.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LockMute {
    /// While APLL2 locks. `MUTE_APLL2_LOCK`, bit 4.
    #[bits(4..=4)]
    apll2: bool,

    /// While the DPLL phase-locks. `MUTE_DPLL_PHLOCK`, bit 2.
    #[bits(2..=2)]
    dpll_phase: bool,

    /// While the DPLL frequency-locks. `MUTE_DPLL_FLLOCK`, bit 1.
    #[bits(1..=1)]
    dpll_frequency: bool,

    /// While APLL1 locks. `MUTE_APLL1_LOCK`, bit 0.
    #[bits(0..=0)]
    apll1: bool,
}
impl Lmk05318Register for LockMute {
    const ADDR: Reg = Reg::LockMute;
}

/// What a muted output drives (`CHn_MUTE_LVL`; the YAML's `x-mute-lvl`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2, closed)]
pub enum MuteLevel {
    /// Muting bypassed: normal operation.
    Bypass = 0,
    /// Differential: P at the common-mode voltage, N low.
    DifferentialVocmPNLow = 1,
    /// Differential: P high, N low.
    DifferentialHighPLowN = 2,
    /// Differential: both low.
    DifferentialLow = 3,
}

/// Four outputs' mute levels: `MUTELVL1`, 0x17, for outputs 3 to 0; `MUTELVL2`, 0x18, for
/// outputs 7 to 4.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MuteLevels {
    /// Output 3 or 7. `CH3_MUTE_LVL`/`CH7_MUTE_LVL`, bits 7:6.
    #[bits(6..=7)]
    fourth: MuteLevel,

    /// Output 2 or 6. Bits 5:4.
    #[bits(4..=5)]
    third: MuteLevel,

    /// Output 1 or 5. Bits 3:2.
    #[bits(2..=3)]
    second: MuteLevel,

    /// Output 0 or 4. Bits 1:0.
    #[bits(0..=1)]
    first: MuteLevel,
}

/// A group of four outputs sharing a [`MuteLevels`] register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MuteGroup {
    /// Outputs 0 to 3.
    Low,
    /// Outputs 4 to 7.
    High,
}

impl IndexedLmk05318Register for MuteLevels {
    type Index = MuteGroup;

    fn addr(group: MuteGroup) -> Reg {
        match group {
            MuteGroup::Low => Reg::MuteLevels0To3,
            MuteGroup::High => Reg::MuteLevels4To7,
        }
    }
}

impl MuteLevels {
    /// All four outputs at `level`.
    pub(crate) fn all(level: MuteLevel) -> Self {
        Self::new()
            .with_fourth(level)
            .with_third(level)
            .with_second(level)
            .with_first(level)
    }
}

/// Clears an output control register's format bits, keeping the rest
/// (`lmk05318_disable_port`'s `regval & ~OUT0_FMT_MSK`).
#[must_use]
pub fn without_format(control: u8) -> u8 {
    u8::from(OutputControl::from(control).with_format(FormatCode::Disabled))
}
