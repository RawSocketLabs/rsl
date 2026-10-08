//! The TX and RX synthesizers (datasheet `TxPLL`, registers 0x10-0x1F, and `RxPLL`,
//! 0x20-0x2F). The two blocks have identical maps 0x10 apart, so each register here is
//! indexed by [`Pll`].

use bnb::{BitEnum, bitfield, u3, u5};

use super::rx_fe::Lna;
use super::spi::BlockReg;
use crate::chips::register::IndexedRegister;

/// Synthesizer register addresses.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
pub(super) enum Reg {
    /// TX PLL `VCO_DIV` (no output-buffer field); see [`VcoSelect`].
    TxVcoSelect = 0x15,
    /// TX PLL `VCO_REG_PFD_U`; see [`VcoRegulator`].
    TxVcoRegulator = 0x17,
    /// RX PLL `VCO_DIV_BUFSEL`.
    RxVcoSelect = 0x25,
    /// RX PLL `VCO_REG_PFD_U`.
    RxVcoRegulator = 0x27,
}

impl BlockReg for Reg {}

/// Which synthesizer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Pll {
    /// The TX synthesizer.
    Tx,
    /// The RX synthesizer.
    Rx,
}

/// The VCO, its divider range, and (RX only) which LNA's LO buffer is driven. Datasheet
/// `VCO_DIV`, register 0x15 (TX), and `VCO_DIV_BUFSEL`, register 0x25 (RX).
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct VcoSelect {
    /// Which of the four VCOs runs. `SELVCO`, bits 7:5.
    #[bits(5..=7)]
    vco: u3,
    /// Output divider range. `FRANGE`, bits 4:2.
    #[bits(2..=4)]
    divider_range: u3,
    /// Which LNA path's LO buffer is powered; RX PLL only. `SELOUT`, bits 1:0; reset LNA1.
    #[bits(0..=1)]
    lo_buffer: Lna,
}

impl IndexedRegister for VcoSelect {
    type Map = Reg;
    type Index = Pll;

    fn addr(pll: Pll) -> Reg {
        match pll {
            Pll::Tx => Reg::TxVcoSelect,
            Pll::Rx => Reg::RxVcoSelect,
        }
    }
}

/// The VCO's supply regulator and the charge pump's up-current offset. Datasheet
/// `VCO_REG_PFD_U`, registers 0x17 (TX) and 0x27 (RX).
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct VcoRegulator {
    /// Bypass the VCO regulator. `BYPVCOREG`, bit 7.
    #[bits(7..=7)]
    regulator_bypassed: bool,
    /// Power the VCO regulator down. `PDVCOREG`, bit 6.
    #[bits(6..=6)]
    regulator_off: bool,
    /// Short the regulator band-gap resistor so it settles faster; disable once charged.
    /// `FSTVCOBG`, bit 5.
    #[bits(5..=5)]
    fast_bandgap_settling: bool,
    /// Charge-pump up-current offset, 10 µA per step. `OFFUP`, bits 4:0.
    #[bits(0..=4)]
    charge_pump_up_offset: u5,
}

impl IndexedRegister for VcoRegulator {
    type Map = Reg;
    type Index = Pll;

    fn addr(pll: Pll) -> Reg {
        match pll {
            Pll::Tx => Reg::TxVcoRegulator,
            Pll::Rx => Reg::RxVcoRegulator,
        }
    }
}
