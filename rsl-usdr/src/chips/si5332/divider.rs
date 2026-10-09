//! The dividers between the PLL and the outputs: the high-speed dividers' ratios, spread
//! spectrum on the interpolative dividers, and powering down the dividers.

use bnb::{bitfield, u2};

use super::reg::Reg;
use crate::chips::register::{IndexedRegister, Register};

/// The interpolative dividers' spread-spectrum banks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SpreadBank {
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
pub(super) struct SpreadSpectrum {
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

/// Powers down dividers (libusdr's `BA_*`). Register 0xBA.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DividerPowerDown {
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

/// The high-speed divider banks the driver programs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HsBank {
    /// Divider 0, bank A: the sample clocks.
    Div0A,

    /// Divider 0, bank B.
    Div0B,

    /// Divider 1, bank A.
    Div1A,

    /// Divider 2, bank A.
    Div2A,

    /// Divider 3, bank A: the mixer LO.
    Div3A,

    /// Divider 3, bank B.
    Div3B,
}

/// A high-speed divider's ratio, dividing the VCO by an integer. `HSDIVnx_DIV`, whole byte;
/// the YAML gives no range or meaning for 0, which libusdr writes to dividers 1 and 2.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct HsDivider {
    /// Divide ratio. Bits 7:0.
    #[bits(0..=7)]
    ratio: u8,
}

impl IndexedRegister for HsDivider {
    type Map = Reg;
    type Index = HsBank;

    fn addr(bank: HsBank) -> Reg {
        match bank {
            HsBank::Div0A => Reg::HsDivider0A,
            HsBank::Div0B => Reg::HsDivider0B,
            HsBank::Div1A => Reg::HsDivider1A,
            HsBank::Div2A => Reg::HsDivider2A,
            HsBank::Div3A => Reg::HsDivider3A,
            HsBank::Div3B => Reg::HsDivider3B,
        }
    }
}
