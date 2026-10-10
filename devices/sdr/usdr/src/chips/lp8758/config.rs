//! The chip-wide configuration register.

use bnb::bitfield;

use super::reg::Reg;
use crate::chips::register::Register;

/// Die-temperature warning level, EN-pin pull-downs, and spread spectrum. `CONFIG`,
/// §7.6.1.23.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Config {
    /// Raise the die-temperature warning at 105 °C instead of 125 °C. `TDIE_WARN_LEVEL`,
    /// bit 3; reset 0.
    #[bits(3..=3)]
    warn_at_105c: bool,

    /// Pull the EN2 pin down. `EN2_PD`, bit 2; reset 1.
    #[bits(2..=2)]
    en2_pull_down: bool,

    /// Pull the EN1 pin down. `EN1_PD`, bit 1; reset 1.
    #[bits(1..=1)]
    en1_pull_down: bool,

    /// Spread the switching frequency to reduce EMI peaks. `EN_SPREAD_SPEC`, bit 0; reset 0.
    #[bits(0..=0)]
    spread_spectrum: bool,
}
impl Register for Config {
    type Map = Reg;
    const ADDR: Reg = Reg::Config;
}
