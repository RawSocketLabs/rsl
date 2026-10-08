//! The second RX variable-gain amplifier (datasheet `RxVGA2`, registers 0x60-0x6F).

use bnb::{BitEnum, bitfield, u4};

use super::spi::BlockReg;
use crate::chips::register::bitfield_register;

/// RXVGA2 register addresses.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
pub(super) enum Reg {
    /// Datasheet `CTRL`; see [`Control`].
    Control = 0x64,
}

impl BlockReg for Reg {}

/// Powers RXVGA2 and sets its output common mode. Datasheet `CTRL`, register 0x64.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Control {
    /// Output common-mode voltage: bit 3 is the sign, bits 2:0 the magnitude in 40 mV
    /// steps. `CMV`, bits 5:2.
    #[bits(2..=5)]
    common_mode: u4,
    /// Power the RXVGA2 modules. `EN`, bit 1; reset 1.
    #[bits(1..=1)]
    enabled: bool,
    /// Take control signals from the test-mode registers instead of decoding them.
    /// `DECODE`, bit 0; reset 0.
    #[bits(0..=0)]
    test_mode_controls: bool,
}
bitfield_register!(Control => Reg::Control);
