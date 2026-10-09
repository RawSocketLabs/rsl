//! The six outputs: each one's source mux, divider, skew, polarity and driver, and
//! powering down the muxes and output buffers.

use bnb::{BitEnum, bitfield, u2, u3, u6};

use super::reg::Reg;
use crate::chips::register::{IndexedRegister, Register};

/// The clock outputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Output {
    /// Output 0: the LMS6002D PLL reference on revision 3, the RX clock on revisions 1-2.
    Out0,

    /// Output 1: the RX clock on revision 3, the PLL reference on revisions 1-2.
    Out1,

    /// Output 2: the TX clock.
    Out2,

    /// Output 3: the external RX mixer's LO.
    Out3,

    /// Output 4: the FPGA transceiver (MGT) reference.
    Out4,

    /// Output 5: the FPGA reference on revisions 3-4, the USB clock on revision 2.
    Out5,
}

/// Implements [`IndexedRegister`] over [`Output`] for a per-output register.
macro_rules! per_output {
    ($ty:ty, [$o0:ident, $o1:ident, $o2:ident, $o3:ident, $o4:ident, $o5:ident]) => {
        impl IndexedRegister for $ty {
            type Map = Reg;
            type Index = Output;

            fn addr(output: Output) -> Reg {
                match output {
                    Output::Out0 => Reg::$o0,
                    Output::Out1 => Reg::$o1,
                    Output::Out2 => Reg::$o2,
                    Output::Out3 => Reg::$o3,
                    Output::Out4 => Reg::$o4,
                    Output::Out5 => Reg::$o5,
                }
            }
        }
    };
}

/// Clocks an output can take without a divider (`OUMUXX_SEL0_*`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
pub(super) enum DirectSource {
    /// The PLL reference before its pre-scaler.
    PllReference,

    /// The PLL reference after its pre-scaler.
    PllReferencePrescaled,

    /// Clock input 2.
    Input2,

    /// Clock input 3.
    Input3,
}

/// Divided clocks an output can take (`OUMUXX_SEL1_*`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u3)]
pub(super) enum DividerSource {
    /// High-speed divider 0.
    HighSpeed0,

    /// High-speed divider 1.
    HighSpeed1,

    /// High-speed divider 2.
    HighSpeed2,

    /// High-speed divider 3.
    HighSpeed3,

    /// High-speed divider 4.
    HighSpeed4,

    /// Interpolative divider 0.
    Interpolative0,

    /// Interpolative divider 1.
    Interpolative1,

    /// No divider: use the direct source. Forced whenever the PLL is off.
    Direct,
}

/// Which clock feeds an output. `OMUXn_SEL10`, 0x25..=0x2A.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OutputSource {
    /// A divider, or [`DividerSource::Direct`] to take `direct`. `OMUXn_SEL1`, bits 6:4.
    #[bits(4..=6)]
    divider: DividerSource,

    /// The undivided source. `OMUXn_SEL0`, bits 1:0.
    #[bits(0..=1)]
    direct: DirectSource,
}
per_output!(
    OutputSource,
    [
        Output0Source,
        Output1Source,
        Output2Source,
        Output3Source,
        Output4Source,
        Output5Source
    ]
);

/// An output's driver type (libusdr's `OUTMODE_*`). `OUTn_MODE`.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8)]
#[repr(u8)]
pub(super) enum DriverMode {
    /// Off.
    Off = 0,

    /// CMOS on the positive pin only.
    CmosPositive = 1,

    /// CMOS on the negative pin only.
    CmosNegative = 2,

    /// CMOS on both pins.
    CmosDual = 3,

    /// 2.5 V/3.3 V LVDS.
    Lvds25 = 4,

    /// 1.8 V LVDS.
    Lvds18 = 5,

    /// 2.5 V/3.3 V LVDS, fast edges.
    Lvds25Fast = 6,

    /// 1.8 V LVDS, fast edges.
    Lvds18Fast = 7,

    /// HCSL, 50 Ω external termination.
    Hcsl50External = 8,

    /// HCSL, 50 Ω internal termination.
    Hcsl50Internal = 9,

    /// HCSL, 42.5 Ω external termination.
    Hcsl42External = 10,

    /// HCSL, 42.5 Ω internal termination.
    Hcsl42Internal = 11,

    /// LVPECL.
    Lvpecl = 12,

    /// Any other value.
    #[catch_all]
    Other(u8),
}
per_output!(
    DriverMode,
    [
        Output0Mode,
        Output1Mode,
        Output2Mode,
        Output3Mode,
        Output4Mode,
        Output5Mode
    ]
);

/// An output's own divider. `OUTn_DIV`.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Divider {
    /// Divide ratio, 1-63; 0 disables the output. Bits 5:0.
    #[bits(0..=5)]
    ratio: u6,
}
per_output!(
    Divider,
    [
        Output0Divider,
        Output1Divider,
        Output2Divider,
        Output3Divider,
        Output4Divider,
        Output5Divider
    ]
);

/// Extra delay on an output. `OUTn_SKEW`.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Skew {
    /// Delay in 35 ps steps, 0-245 ps (the YAML says "up to 280 ps", which three bits
    /// cannot reach). Bits 2:0.
    #[bits(0..=2)]
    steps: u3,
}
per_output!(
    Skew,
    [
        Output0Skew,
        Output1Skew,
        Output2Skew,
        Output3Skew,
        Output4Skew,
        Output5Skew
    ]
);

/// Output pin polarity in dual-CMOS mode, and the stopped state. `OUTn_CMOS_INV_Z`.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Polarity {
    /// Polarity of the two dual-CMOS pins. `OUTn_CMOS_INV`, bits 5:4.
    #[bits(4..=5)]
    cmos_inversion: u2,

    /// Go high-impedance when stopped. `OUTn_STOP_HIGHZ`, bit 0.
    #[bits(0..=0)]
    high_z_when_stopped: bool,
}
per_output!(
    Polarity,
    [
        Output0Polarity,
        Output1Polarity,
        Output2Polarity,
        Output3Polarity,
        Output4Polarity,
        Output5Polarity
    ]
);

/// CMOS output edge rate and impedance. `OUTn_CMOS_SLEW`.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CmosDrive {
    /// Selects the CMOS output impedance; the YAML does not say which value is which, and
    /// the driver writes 0. `OUTn_CMOS_STR`, bit 2.
    #[bits(2..=2)]
    impedance_select: bool,

    /// Slew rate, fast (0) to slow (3). `OUTn_CMOS_SLEW`, bits 1:0.
    #[bits(0..=1)]
    slew: u2,
}
per_output!(
    CmosDrive,
    [
        Output0Drive,
        Output1Drive,
        Output2Drive,
        Output3Drive,
        Output4Drive,
        Output5Drive
    ]
);

/// Powers down output source selectors (libusdr's `BB_*`). Register 0xBB.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SourcePowerDown {
    /// Output 5's selector. `OMUX5_DIS`, bit 5.
    #[bits(5..=5)]
    output5: bool,

    /// Output 4's selector. `OMUX4_DIS`, bit 4.
    #[bits(4..=4)]
    output4: bool,

    /// Output 3's selector. `OMUX3_DIS`, bit 3.
    #[bits(3..=3)]
    output3: bool,

    /// Output 2's selector. `OMUX2_DIS`, bit 2.
    #[bits(2..=2)]
    output2: bool,

    /// Output 1's selector. `OMUX1_DIS`, bit 1.
    #[bits(1..=1)]
    output1: bool,

    /// Output 0's selector. `OMUX0_DIS`, bit 0.
    #[bits(0..=0)]
    output0: bool,
}
impl Register for SourcePowerDown {
    type Map = Reg;
    const ADDR: Reg = Reg::SourcePowerDown;
}

/// Powers down output buffers 0..=3 (libusdr's `BC_*`). Register 0xBC.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Output0to3PowerDown {
    /// Output 3. `OUT3_DIS`, bit 6.
    #[bits(6..=6)]
    output3: bool,

    /// Output 2. `OUT2_DIS`, bit 3.
    #[bits(3..=3)]
    output2: bool,

    /// Output 1. `OUT1_DIS`, bit 1.
    #[bits(1..=1)]
    output1: bool,

    /// Output 0. `OUT0_DIS`, bit 0.
    #[bits(0..=0)]
    output0: bool,
}
impl Register for Output0to3PowerDown {
    type Map = Reg;
    const ADDR: Reg = Reg::Output0to3PowerDown;
}

/// Powers down output buffers 4 and 5 (libusdr's `BD_*`). Register 0xBD.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Output4and5PowerDown {
    /// Output 5. `OUT5_DIS`, bit 2.
    #[bits(2..=2)]
    output5: bool,

    /// Output 4. `OUT4_DIS`, bit 1.
    #[bits(1..=1)]
    output4: bool,
}
impl Register for Output4and5PowerDown {
    type Map = Reg;
    const ADDR: Reg = Reg::Output4and5PowerDown;
}
