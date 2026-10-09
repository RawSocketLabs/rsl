//! The input stage: which reference feeds the PLL, how the clock inputs are received, the
//! crystal's load, and powering down the input-side blocks.

use bnb::{BitEnum, bitfield, u6};

use super::reg::Reg;
use crate::chips::register::{IndexedRegister, Register};

/// The PLL's reference input. `IMUX_SEL`, 0x24 (libusdr's `IMUX_*` values).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8)]
#[repr(u8)]
pub(super) enum PllReference {
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
pub(super) enum InputMode {
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

/// Powers down input-side blocks (libusdr's `B9_*`). Register 0xB9.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InputPowerDown {
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

/// Extra crystal load capacitance. `XOSC_CINT_ENA`, 0xBF.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CrystalLoad {
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
pub(super) enum CrystalPin {
    /// Pin XA.
    Xa,

    /// Pin XB.
    Xb,
}

/// Load-capacitance trim on one crystal pin. `XOSC_CTRIM_XA`/`XOSC_CTRIM_XB`.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CrystalTrim {
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
