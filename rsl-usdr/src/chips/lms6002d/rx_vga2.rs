//! The second RX variable-gain amplifier (datasheet `RxVGA2`, registers 0x60-0x6F).
//!
//! RXVGA2 is the last gain stage before the ADC, after the channel filter, 0 to 30 dB in
//! libusdr's range, and so sets how fully the signal fills the ADC's input range.
//! Power-up writes its output common-mode voltage ([`Control`]) to the value from Lime's
//! LMS6002D FAQ, 5.27.

use bnb::{BitEnum, bitfield, u4};

use super::spi::BlockReg;
use crate::chips::register::Register;

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
impl Register for Control {
    type Map = Reg;
    const ADDR: Reg = Reg::Control;
}

impl Control {
    /// Lime's recommended setting, 0x36, from the LMS6002D FAQ v1.0r12, 5.27 (as libusdr
    /// writes it): powered, common-mode code `0b1101`. libusdr's YAML scale cannot place
    /// that code: 40 mV steps up from its 620 mV at `0b1000` give 820 mV, but down from its
    /// 860 mV at `0b1111` give 780 mV.
    pub(super) const LIME_RECOMMENDED: Self = Self::new()
        .with_common_mode(u4::new(0b1101))
        .with_enabled(true);
}
