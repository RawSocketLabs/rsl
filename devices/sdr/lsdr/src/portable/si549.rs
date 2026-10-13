//! Skyworks (Silicon Labs) Si549 oscillator, the ADCs' sample clock: the register libusdr's
//! `hw/si549/si549.c` gates the output with (`si549_enable`).
//!
//! Field names and meanings from the vendored `hw/si549/si549.yaml`. The frequency plan
//! (`si549_set_freq`, `/dm/rate/master`) is not ported. Chip-level code: it moves to a
//! shared `devices/sdr/chips/si549` crate when a second driver needs it (`PORTING.md`).

use bnb::{BitEnum, bitfield};
use rsl_lowlevel::Register;

/// The chip's name in errors.
pub(crate) const NAME: &str = "Si549";

/// The registers the `LSDr` touches.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
pub(crate) enum Reg {
    /// See [`OutputControl`]. `ODC`, register 17.
    OutputControl = 17,
}

/// Drives or gates the output clock. `ODC`.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OutputControl {
    /// Drive the output clock; clear disables it. `OE`, bit 0.
    #[bits(0..=0)]
    enabled: bool,
}

impl Register for OutputControl {
    type Map = Reg;
    const ADDR: Reg = Reg::OutputControl;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gating_the_output_writes_libusdrs_bytes_to_odc() {
        let entry = |enabled| OutputControl::new().with_enabled(enabled).entry();
        assert_eq!(entry(false), (Reg::OutputControl, 0));
        assert_eq!(entry(true), (Reg::OutputControl, 1));
    }
}
