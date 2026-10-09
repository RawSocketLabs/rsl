//! Si5332 clock generator (source: `hw/si5332/si5332.c`; register meanings from libusdr's
//! `hw/si5332/si5332.yaml`, vendored under `oracle/libusdr`).
//!
//! On the uSDR it turns the reference into the PLL, RX and TX clocks, and drives the
//! external RX mixer's LO on output 3.
//!
//! # What it does
//!
//! The Si5332 takes one reference clock and produces up to six output clocks, each at its
//! own frequency and logic standard. A clock passes through these stages:
//!
//! 1. **Input.** The reference comes from the crystal-oscillator pins or a clock input;
//!    [`PllReference`] picks which feeds the PLL. [`InputMode`] sets how clock input 2 is
//!    received.
//! 2. **PLL.** The PLL locks its VCO, somewhere in 2.375–2.625 GHz, to the reference.
//! 3. **Dividers.** Five high-speed dividers split the VCO by integers. Two interpolative
//!    dividers split it by fractions, and can add spread spectrum ([`SpreadSpectrum`]).
//! 4. **Output mux.** Each output takes either a divider's clock or, bypassing the PLL,
//!    the reference itself ([`OutputSource`]).
//! 5. **Output stage.** Each output divides again ([`Divider`]), can be delayed
//!    ([`Skew`]) or inverted ([`Polarity`]), and drives its pins as CMOS, LVDS, LVPECL or
//!    HCSL ([`DriverMode`], [`CmosDrive`]).
//!
//! Unused stages can be powered down ([`InputPowerDown`], [`DividerPowerDown`], [`SourcePowerDown`]
//! and the output power-down registers).
//!
//! # The READY/ACTIVE state machine
//!
//! The chip runs in one of two states, requested through [`RequestedState`] and reported
//! in [`CurrentState`]: READY, for changing the configuration, and ACTIVE, for running
//! it. libusdr's init and sample-rate plans request READY, write, then request ACTIVE,
//! and poll the state until it reads READY, ACTIVE or "no input clock". Not verified
//! here: what the outputs do while the chip is in READY. If they stop, every such
//! change briefly interrupts the LMS6002D's PLL reference and the sample clocks.
//!
//! That matters for the band-crossing problem. `si5332_set_port3_en`, which libusdr
//! calls when the RX path moves into or out of the board-mixer path, writes the output
//! enables, then requests READY, writes the power-down registers and requests ACTIVE,
//! without polling. It gates output 3 (the mixer LO) by the mixer state and output 2 (the
//! TX clock) by whether TX runs. The suspected cause of the band-crossing failures is
//! that READY cycle; the planned fix gates output 3 without leaving ACTIVE, and will be
//! checked against the oracle and hardware.
//!
//! # How the driver uses it
//!
//! At power-up, [`Si5332::init`] checks a Si5332 answers, then writes a plan that routes
//! the reference straight to outputs 0 to 2 and turns the rest off; the PLL is not yet
//! used. On revision 3 the reference oscillator starts only after this, so the chip may
//! report no input clock; the board sequence tolerates exactly that error. Setting a
//! sample rate (not yet ported) will usually move the sample clocks onto PLL dividers.

use std::time::Duration;

use bnb::{BitEnum, bitfield, u2, u3, u6};

use super::register::{I2cRegisters, IndexedRegister, Register};
use crate::error::Error;
use crate::lowlevel::{Bus, I2cAddr};

/// Register addresses, with libusdr's C names (`si5332.c`) in the docs.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
enum Reg {
    /// Supply status flags (`VDDO_OK`, `VDD_XTAL_OK`), read for diagnostics only. `VDD_OK`,
    /// 0x05.
    SupplyStatus = 0x05,

    /// See [`RequestedState`]. `USYS_CTRL`, 0x06.
    RequestedState = 0x06,

    /// See [`CurrentState`]. `USYS_STAT`, 0x07.
    CurrentState = 0x07,

    /// Device part number. `DEVICE_PN_BASE`, 0x0D.
    DevicePn = 0x0d,

    /// Device revision. `DEVICE_REV`, 0x0E.
    DeviceRev = 0x0e,

    /// Device grade. `DEVICE_GRADE`, 0x0F.
    DeviceGrade = 0x0f,

    /// Orderable part number, characters 1 and 0. `FACTORY_OPN_ID10`, 0x10.
    FactoryOpnId10 = 0x10,

    /// Orderable part number, characters 3 and 2. `FACTORY_OPN_ID32`, 0x11.
    FactoryOpnId32 = 0x11,

    /// Orderable part number, revision and character 4. `FACTORY_OPN_IDR4`, 0x12.
    FactoryOpnIdR4 = 0x12,

    /// Design ID, byte 0. `DESIGN_ID0`, 0x13.
    DesignId0 = 0x13,

    /// Design ID, byte 1. `DESIGN_ID1`, 0x14.
    DesignId1 = 0x14,

    /// Design ID, byte 2. `DESIGN_ID2`, 0x15.
    DesignId2 = 0x15,

    /// See [`PllReference`]. `IMUX_SEL`, 0x24.
    PllReference = 0x24,

    /// See [`OutputSource`], output 0. `OMUX0_SEL10`, 0x25.
    Output0Source = 0x25,

    /// Output 1 source. `OMUX1_SEL10`, 0x26.
    Output1Source = 0x26,

    /// Output 2 source. `OMUX2_SEL10`, 0x27.
    Output2Source = 0x27,

    /// Output 3 source. `OMUX3_SEL10`, 0x28.
    Output3Source = 0x28,

    /// Output 4 source. `OMUX4_SEL10`, 0x29.
    Output4Source = 0x29,

    /// Output 5 source. `OMUX5_SEL10`, 0x2A.
    Output5Source = 0x2a,

    /// See [`SpreadSpectrum`], divider 0 bank A. `ID0A_SS`, 0x3C.
    Id0aSpreadSpectrum = 0x3c,

    /// Divider 0 bank B spread spectrum. `ID0B_SS`, 0x48.
    Id0bSpreadSpectrum = 0x48,

    /// Divider 1 bank A spread spectrum. `ID1A_SS`, 0x54.
    Id1aSpreadSpectrum = 0x54,

    /// Divider 1 bank B spread spectrum. `ID1B_SS`, 0x60.
    Id1bSpreadSpectrum = 0x60,

    /// See [`InputMode`]. `CLKIN_2_CLK_SEL`, 0x73.
    Input2Mode = 0x73,

    /// Input buffer 3's mode; libusdr's map has no layout, and it writes 0.
    /// `CLKIN_3_CLK_SEL`, 0x74.
    Input3Mode = 0x74,

    /// See [`DriverMode`], output 0. `OUT0_MODE`, 0x7A.
    Output0Mode = 0x7a,

    /// See [`Divider`], output 0. `OUT0_DIV`, 0x7B.
    Output0Divider = 0x7b,

    /// See [`Skew`], output 0. `OUT0_SKEW`, 0x7C.
    Output0Skew = 0x7c,

    /// See [`Polarity`], output 0. `OUT0_CMOS_INV_Z`, 0x7D.
    Output0Polarity = 0x7d,

    /// See [`CmosDrive`], output 0. `OUT0_CMOS_SLEW`, 0x7E.
    Output0Drive = 0x7e,

    /// Output 1 driver mode. `OUT1_MODE`, 0x7F.
    Output1Mode = 0x7f,

    /// Output 1 divider. `OUT1_DIV`, 0x80.
    Output1Divider = 0x80,

    /// Output 1 skew. `OUT1_SKEW`, 0x81.
    Output1Skew = 0x81,

    /// Output 1 polarity. `OUT1_CMOS_INV_Z`, 0x82.
    Output1Polarity = 0x82,

    /// Output 1 CMOS drive. `OUT1_CMOS_SLEW`, 0x83.
    Output1Drive = 0x83,

    /// Output 2 driver mode. `OUT2_MODE`, 0x89.
    Output2Mode = 0x89,

    /// Output 2 divider. `OUT2_DIV`, 0x8A.
    Output2Divider = 0x8a,

    /// Output 2 skew. `OUT2_SKEW`, 0x8B.
    Output2Skew = 0x8b,

    /// Output 2 polarity. `OUT2_CMOS_INV_Z`, 0x8C.
    Output2Polarity = 0x8c,

    /// Output 2 CMOS drive. `OUT2_CMOS_SLEW`, 0x8D.
    Output2Drive = 0x8d,

    /// Output 3 driver mode. `OUT3_MODE`, 0x98.
    Output3Mode = 0x98,

    /// Output 3 divider. `OUT3_DIV`, 0x99.
    Output3Divider = 0x99,

    /// Output 3 skew. `OUT3_SKEW`, 0x9A.
    Output3Skew = 0x9a,

    /// Output 3 polarity. `OUT3_CMOS_INV_Z`, 0x9B.
    Output3Polarity = 0x9b,

    /// Output 3 CMOS drive. `OUT3_CMOS_SLEW`, 0x9C.
    Output3Drive = 0x9c,

    /// Output 4 driver mode. `OUT4_MODE`, 0xA7.
    Output4Mode = 0xa7,

    /// Output 4 divider. `OUT4_DIV`, 0xA8.
    Output4Divider = 0xa8,

    /// Output 4 skew. `OUT4_SKEW`, 0xA9.
    Output4Skew = 0xa9,

    /// Output 4 polarity. `OUT4_CMOS_INV_Z`, 0xAA.
    Output4Polarity = 0xaa,

    /// Output 4 CMOS drive. `OUT4_CMOS_SLEW`, 0xAB.
    Output4Drive = 0xab,

    /// Output 5 driver mode. `OUT5_MODE`, 0xAC.
    Output5Mode = 0xac,

    /// Output 5 divider. `OUT5_DIV`, 0xAD.
    Output5Divider = 0xad,

    /// Output 5 skew. `OUT5_SKEW`, 0xAE.
    Output5Skew = 0xae,

    /// Output 5 polarity. `OUT5_CMOS_INV_Z`, 0xAF.
    Output5Polarity = 0xaf,

    /// Output 5 CMOS drive. `OUT5_CMOS_SLEW`, 0xB0.
    Output5Drive = 0xb0,

    /// Output enables for outputs 0..=3 (bits 0, 1, 3, 6). libusdr writes 0xFF, which also
    /// sets the undefined bits, so it stays a raw byte. `OUT3210_OE`, 0xB6.
    OutputEnables0to3 = 0xb6,

    /// Output enables for outputs 4 and 5 (bits 1, 2); written as 0xFF like
    /// [`Reg::OutputEnables0to3`]. `OUT54_OE`, 0xB7.
    OutputEnables4and5 = 0xb7,

    /// See [`InputPowerDown`]. 0xB9.
    InputPowerDown = 0xb9,

    /// See [`DividerPowerDown`]. 0xBA.
    DividerPowerDown = 0xba,

    /// See [`SourcePowerDown`]. 0xBB.
    SourcePowerDown = 0xbb,

    /// See [`Output0to3PowerDown`]. 0xBC.
    Output0to3PowerDown = 0xbc,

    /// See [`Output4and5PowerDown`]. 0xBD.
    Output4and5PowerDown = 0xbd,

    /// See [`CrystalLoad`]. `XOSC_CINT_ENA`, 0xBF.
    CrystalLoad = 0xbf,

    /// See [`CrystalTrim`], pin XA. `XOSC_CTRIM_XA`, 0xC0.
    CrystalTrimXa = 0xc0,

    /// Crystal trim, pin XB. `XOSC_CTRIM_XB`, 0xC1.
    CrystalTrimXb = 0xc1,
}

/// The nine identification registers, in address order.
const ID_REGS: [Reg; 9] = [
    Reg::DevicePn,
    Reg::DeviceRev,
    Reg::DeviceGrade,
    Reg::FactoryOpnId10,
    Reg::FactoryOpnId32,
    Reg::FactoryOpnIdR4,
    Reg::DesignId0,
    Reg::DesignId1,
    Reg::DesignId2,
];

/// The operating state the device is commanded into (write-only). `USYS_CTRL`, 0x06.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8)]
#[repr(u8)]
enum RequestedState {
    /// Hold the outputs so the configuration can be changed.
    Ready = 0x01,

    /// Run with the current configuration.
    Active = 0x02,

    /// Any other value.
    #[catch_all]
    Other(u8),
}
impl Register for RequestedState {
    type Map = Reg;
    const ADDR: Reg = Reg::RequestedState;
}

/// The state the device is in (read-only). `USYS_STAT`, 0x07.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8)]
#[repr(u8)]
enum CurrentState {
    /// Outputs held; configuration can be changed.
    Ready = 0x01,

    /// Running.
    Active = 0x02,

    /// No input clock detected, so it cannot become active.
    NoInputClock = 0x89,

    /// Any other value, including while a transition is in progress.
    #[catch_all]
    Other(u8),
}
impl Register for CurrentState {
    type Map = Reg;
    const ADDR: Reg = Reg::CurrentState;
}

/// The PLL's reference input. `IMUX_SEL`, 0x24 (libusdr's `IMUX_*` values).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8)]
#[repr(u8)]
enum PllReference {
    /// The crystal or on-board oscillator.
    Oscillator = 1,

    /// Clock input 2.
    Input2 = 2,

    /// Any other value.
    #[catch_all]
    Other(u8),
}
impl Register for PllReference {
    type Map = Reg;
    const ADDR: Reg = Reg::PllReference;
}

/// Clock input 2's buffer mode. `CLKIN_2_CLK_SEL`, 0x73 (libusdr's `IMUX_INX_*`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8)]
#[repr(u8)]
enum InputMode {
    /// Buffer off.
    Off = 0,

    /// Differential input.
    Differential = 1,

    /// Any other value.
    #[catch_all]
    Other(u8),
}
impl Register for InputMode {
    type Map = Reg;
    const ADDR: Reg = Reg::Input2Mode;
}

/// The clock outputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Output {
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
enum DirectSource {
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
enum DividerSource {
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
struct OutputSource {
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
enum DriverMode {
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
struct Divider {
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
struct Skew {
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
struct Polarity {
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
struct CmosDrive {
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

/// The interpolative dividers' spread-spectrum banks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SpreadBank {
    /// Divider 0, bank A.
    Id0a,

    /// Divider 0, bank B.
    Id0b,

    /// Divider 1, bank A.
    Id1a,

    /// Divider 1, bank B.
    Id1b,
}

/// Spread-spectrum modulation of an interpolative divider bank; the one bank field that may
/// change while the bank is active. `IDnx_SS`.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SpreadSpectrum {
    /// Spread-spectrum mode. `SS_MODE`, bits 2:1.
    #[bits(1..=2)]
    mode: u2,

    /// Modulate the divider. `SS_ENA`, bit 0.
    #[bits(0..=0)]
    enabled: bool,
}

impl IndexedRegister for SpreadSpectrum {
    type Map = Reg;
    type Index = SpreadBank;

    fn addr(bank: SpreadBank) -> Reg {
        match bank {
            SpreadBank::Id0a => Reg::Id0aSpreadSpectrum,
            SpreadBank::Id0b => Reg::Id0bSpreadSpectrum,
            SpreadBank::Id1a => Reg::Id1aSpreadSpectrum,
            SpreadBank::Id1b => Reg::Id1bSpreadSpectrum,
        }
    }
}

/// Powers down input-side blocks (libusdr's `B9_*`). Register 0xB9.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct InputPowerDown {
    /// The PLL. `PLL_DIS`, bit 5.
    #[bits(5..=5)]
    pll: bool,

    /// The pre-divider buffer. `PDIV_DIS`, bit 4.
    #[bits(4..=4)]
    pre_divider: bool,

    /// The input mux. `IMUX_DIS`, bit 3.
    #[bits(3..=3)]
    input_mux: bool,

    /// Input buffer 0. `IBUF0_DIS`, bit 1.
    #[bits(1..=1)]
    input_buffer0: bool,

    /// The crystal oscillator buffer. `XOSC_DIS`, bit 0.
    #[bits(0..=0)]
    oscillator: bool,
}
impl Register for InputPowerDown {
    type Map = Reg;
    const ADDR: Reg = Reg::InputPowerDown;
}

/// Powers down dividers (libusdr's `BA_*`). Register 0xBA.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DividerPowerDown {
    /// Interpolative divider 1. `ID1_DIS`, bit 6.
    #[bits(6..=6)]
    interpolative1: bool,

    /// Interpolative divider 0. `ID0_DIS`, bit 5.
    #[bits(5..=5)]
    interpolative0: bool,

    /// High-speed divider 4. `HSDIV4_DIS`, bit 4.
    #[bits(4..=4)]
    high_speed4: bool,

    /// High-speed divider 3. `HSDIV3_DIS`, bit 3.
    #[bits(3..=3)]
    high_speed3: bool,

    /// High-speed divider 2. `HSDIV2_DIS`, bit 2.
    #[bits(2..=2)]
    high_speed2: bool,

    /// High-speed divider 1. `HSDIV1_DIS`, bit 1.
    #[bits(1..=1)]
    high_speed1: bool,

    /// High-speed divider 0. `HSDIV0_DIS`, bit 0.
    #[bits(0..=0)]
    high_speed0: bool,
}
impl Register for DividerPowerDown {
    type Map = Reg;
    const ADDR: Reg = Reg::DividerPowerDown;
}

/// Powers down output source selectors (libusdr's `BB_*`). Register 0xBB.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SourcePowerDown {
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
struct Output0to3PowerDown {
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
struct Output4and5PowerDown {
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

/// Extra crystal load capacitance. `XOSC_CINT_ENA`, 0xBF.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CrystalLoad {
    /// Add a fixed 8 pF on both crystal pins. Bit 7.
    #[bits(7..=7)]
    extra_8pf: bool,
}
impl Register for CrystalLoad {
    type Map = Reg;
    const ADDR: Reg = Reg::CrystalLoad;
}

/// A crystal pin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CrystalPin {
    /// Pin XA.
    Xa,

    /// Pin XB.
    Xb,
}

/// Load-capacitance trim on one crystal pin. `XOSC_CTRIM_XA`/`XOSC_CTRIM_XB`.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CrystalTrim {
    /// Trim code. Bits 5:0.
    #[bits(0..=5)]
    capacitance: u6,
}

impl IndexedRegister for CrystalTrim {
    type Map = Reg;
    type Index = CrystalPin;

    fn addr(pin: CrystalPin) -> Reg {
        match pin {
            CrystalPin::Xa => Reg::CrystalTrimXa,
            CrystalPin::Xb => Reg::CrystalTrimXb,
        }
    }
}

/// Polls of [`CurrentState`] before giving up, 10 µs apart (`si5532_get_state`).
const STATE_POLLS: usize = 100;

/// Where the reference comes from: the clock every output is derived from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reference {
    /// The on-board 26 MHz oscillator, which revision 3 carries and the FPGA switches on.
    Oscillator,

    /// Clock input 2, received differentially: the reference on revisions 1 and 2.
    Input2,
}

/// Which of outputs 0 and 1 drives LVPECL; the other drives CMOS.
///
/// The LVPECL output carries the LMS6002D's PLL reference, and the CMOS one the RX sample
/// clock. Revision 3 swapped the two: output 0 is the PLL reference there, output 1 on
/// revisions 1 and 2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LvpeclOutput {
    /// Output 0 is LVPECL.
    Out0,

    /// Output 1 is LVPECL.
    Out1,
}

/// One register write: address and byte.
type RegWrite = (Reg, u8);

/// A Si5332 on the I2C bus.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Si5332 {
    /// The chip's registers.
    regs: I2cRegisters,
}

impl Si5332 {
    /// The clock generator at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self {
            regs: I2cRegisters::new(dev, "Si5332 read", "Si5332 write"),
        }
    }

    /// The clock generator where the uSDR wires it: FPGA I2C bus 0, address 0x6A
    /// (`I2C_DEV_CLKGEN` in libusdr).
    pub(crate) const fn usdr() -> Self {
        Self::at(I2cAddr::new(0, 0x6a))
    }

    /// Identifies the chip and programs its power-up output plan (`si5332_init`), with
    /// output 0 divided by `div`.
    pub(crate) fn init(
        self,
        bus: &mut dyn Bus,
        div: u8,
        reference: Reference,
        lvpecl: LvpeclOutput,
    ) -> Result<(), Error> {
        let mut id = [0; ID_REGS.len()];
        for (reg, byte) in ID_REGS.into_iter().zip(&mut id) {
            *byte = self.regs.read_raw(bus, reg)?;
        }
        self.regs.read_raw(bus, Reg::SupplyStatus)?;
        if id[..6].iter().all(|&byte| byte == 0xff) {
            return Err(Error::ChipMissing("Si5332"));
        }

        self.wait_state(bus, false)?;
        for (reg, value) in Self::power_up_plan(div, reference, lvpecl) {
            self.regs.write_raw(bus, reg, value)?;
        }
        // libusdr reads FPGA register 0xC here and ignores the result; kept so traces line up.
        let _ = bus.read_regs(0xc, &mut [0]);
        self.wait_state(bus, true)
    }

    /// The register writes of `si5332_init`, in order: hold the outputs, route every output
    /// straight from the reference, set each output's driver, power down unused blocks, run.
    fn power_up_plan(div: u8, reference: Reference, lvpecl: LvpeclOutput) -> [RegWrite; 49] {
        let (pll_reference, input2, inputs_off) = match reference {
            Reference::Oscillator => (
                PllReference::Oscillator,
                InputMode::Off,
                InputPowerDown::new().with_input_buffer0(true),
            ),
            Reference::Input2 => (
                PllReference::Input2,
                InputMode::Differential,
                InputPowerDown::new().with_oscillator(true),
            ),
        };
        let (out0_mode, out1_mode) = match lvpecl {
            LvpeclOutput::Out0 => (DriverMode::Lvpecl, DriverMode::CmosPositive),
            LvpeclOutput::Out1 => (DriverMode::CmosPositive, DriverMode::Lvpecl),
        };
        let reference_direct = OutputSource::new()
            .with_divider(DividerSource::Direct)
            .with_direct(DirectSource::PllReference);
        let divide_by = |ratio| Divider::new().with_ratio(u6::new(ratio));
        let slow_cmos = CmosDrive::new().with_slew(u2::new(3));
        let no_spread = SpreadSpectrum::new();
        let dividers_off = DividerPowerDown::new()
            .with_high_speed1(true)
            .with_high_speed2(true)
            .with_high_speed4(true)
            .with_interpolative0(true)
            .with_interpolative1(true);

        #[rustfmt::skip]
        let plan = [
            RequestedState::Ready.entry(),
            pll_reference.entry(),
            input2.entry(),
            (Reg::Input3Mode, 0),
            no_spread.entry_at(SpreadBank::Id0a),
            no_spread.entry_at(SpreadBank::Id0b),
            no_spread.entry_at(SpreadBank::Id1a),
            no_spread.entry_at(SpreadBank::Id1b),
            reference_direct.entry_at(Output::Out0),
            reference_direct.entry_at(Output::Out1),
            reference_direct.entry_at(Output::Out2),
            reference_direct.entry_at(Output::Out3),
            reference_direct.entry_at(Output::Out4),
            reference_direct.entry_at(Output::Out5),
            out0_mode.entry_at(Output::Out0),
            Divider::new().with_ratio(u6::new(div)).entry_at(Output::Out0),
            Skew::new().entry_at(Output::Out0),
            Polarity::new().entry_at(Output::Out0),
            slow_cmos.entry_at(Output::Out0),
            out1_mode.entry_at(Output::Out1),
            divide_by(1).entry_at(Output::Out1),
            Skew::new().entry_at(Output::Out1),
            Polarity::new().entry_at(Output::Out1),
            slow_cmos.entry_at(Output::Out1),
            DriverMode::CmosPositive.entry_at(Output::Out2),
            divide_by(1).entry_at(Output::Out2),
            Skew::new().entry_at(Output::Out2),
            Polarity::new().entry_at(Output::Out2),
            slow_cmos.entry_at(Output::Out2),
            divide_by(0).entry_at(Output::Out3),
            divide_by(0).entry_at(Output::Out4),
            DriverMode::Off.entry_at(Output::Out5),
            divide_by(1).entry_at(Output::Out5),
            Skew::new().entry_at(Output::Out5),
            Polarity::new().entry_at(Output::Out5),
            slow_cmos.entry_at(Output::Out5),
            DriverMode::Off.entry_at(Output::Out3),
            DriverMode::Off.entry_at(Output::Out4),
            (Reg::OutputEnables0to3, 0xff),
            (Reg::OutputEnables4and5, 0xff),
            inputs_off.entry(),
            dividers_off.entry(),
            SourcePowerDown::new().with_output4(true).with_output5(true).entry(),
            Output0to3PowerDown::new().entry(),
            Output4and5PowerDown::new().with_output4(true).with_output5(true).entry(),
            CrystalLoad::new().entry(),
            CrystalTrim::new().entry_at(CrystalPin::Xa),
            CrystalTrim::new().entry_at(CrystalPin::Xb),
            RequestedState::Active.entry(),
        ];
        plan
    }

    /// Polls [`CurrentState`] until it settles (`si5532_get_state`).
    ///
    /// Like libusdr, a state that never settles is not an error, and a missing input clock
    /// is one only once programming is done (`after_init`).
    fn wait_state(self, bus: &mut dyn Bus, after_init: bool) -> Result<(), Error> {
        let mut state = CurrentState::Other(0);
        for _ in 0..STATE_POLLS {
            bus.sleep(Duration::from_micros(10));
            state = self.regs.read(bus)?;
            if !matches!(state, CurrentState::Other(_)) {
                break;
            }
        }
        match state {
            CurrentState::NoInputClock if after_init => Err(Error::ClockInputMissing),
            _ => Ok(()),
        }
    }
}
