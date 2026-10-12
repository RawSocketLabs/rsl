//! The TX RF section (datasheet `TRF`, registers 0x40-0x4F): the two variable-gain
//! amplifiers, the power amplifiers and their drive.
//!
//! The transmit chain's analog stages after the filter: TXVGA1 sets the baseband level into
//! the TX mixer, TXVGA2 the RF level after it, and one of two on-chip amplifiers drives the
//! board's TX balun. Each amplifier is matched to its own band, so the board's TX switch has
//! to follow the [`PowerAmp`] choice. Power-up selects PA1 so the chip idles in a known
//! state.

use bnb::{BitEnum, bitfield, u2, u4, u5};

use super::spi::BlockReg;
use crate::chips::register::Register;

/// TX RF register addresses.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
pub(super) enum Reg {
    /// Datasheet `VGA1GAIN`; see [`Vga1Gain`].
    Vga1Gain = 0x41,

    /// Datasheet `PA_CTRL`; see [`PaSelect`].
    PaSelect = 0x44,

    /// Datasheet `CTRL2`; see [`Vga2Gain`].
    Vga2Gain = 0x45,

    /// Datasheet `CTRL4`; see [`Bias`].
    Bias = 0x47,
}

impl BlockReg for Reg {}

/// A TX output amplifier.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
pub(crate) enum PowerAmp {
    /// Both amplifiers off.
    Off,

    /// PA1: up to 2.8 GHz, through the B0322J5050AHF balun, TX switch 1. libusdr's `W`
    /// path and the power-up default.
    Pa1,

    /// PA2: up to 3.8 GHz, through the 3600BL14M050 balun, TX switch 0. libusdr's `H`
    /// path.
    Pa2,

    /// Both amplifiers off: the datasheet's other off encoding, which libusdr uses as its
    /// AUX path (`TXPATH_AUX`).
    OffAlt,
}

/// Which TX amplifier drives the output, and the auxiliary loopback amplifier. Datasheet
/// `PA_CTRL`, register 0x44.
///
/// libusdr sets `aux_pa_off` when selecting its AUX path, the opposite of the YAML's
/// description; the driver only ever clears it.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PaSelect {
    /// The active amplifier. `EN12`, bits 4:3.
    #[bits(3..=4)]
    amplifier: PowerAmp,

    /// Power the auxiliary (RF loopback) amplifier down. `ENAUX`, bit 2; reset 0.
    #[bits(2..=2)]
    aux_pa_off: bool,
}
impl Register for PaSelect {
    type Map = Reg;
    const ADDR: Reg = Reg::PaSelect;
}

/// TX LO buffer current and amplifier cascode bias, which trade current for linearity.
/// Datasheet `CTRL4`, register 0x47.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Bias {
    /// LO buffer bias current, 5/6 mA per step; more is more linear. `ICT_TXLOBUF`, bits
    /// 7:4; reset 6.
    #[bits(4..=7)]
    lo_buffer_current: u4,

    /// Lowers the amplifiers' cascode base voltage as it rises. `VBCAS_TXDRV`, bits 3:0;
    /// reset 0 (maximum base voltage).
    #[bits(0..=3)]
    cascode_bias: u4,
}
impl Register for Bias {
    type Map = Reg;
    const ADDR: Reg = Reg::Bias;
}

impl Bias {
    /// Lime's recommended setting, 0x40, from the LMS6002D FAQ v1.0r12, 5.27 (as libusdr
    /// writes it): LO buffer current code 4, about 3.3 mA against the 5 mA reset, and the
    /// cascode bias left at its reset value.
    pub(super) const LIME_RECOMMENDED: Self = Self::new().with_lo_buffer_current(u4::new(4));
}

/// TXVGA1's gain, the baseband level into the TX mixer. Datasheet `VGA1GAIN`, register
/// 0x41.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Vga1Gain {
    /// The gain code, log-linear in 1 dB steps from -35 dB (code 0) to -4 dB (code 31).
    /// `VGA1GAIN`, bits 4:0.
    #[bits(0..=4)]
    code: u5,
}
impl Register for Vga1Gain {
    type Map = Reg;
    const ADDR: Reg = Reg::Vga1Gain;
}

impl Vga1Gain {
    /// The lowest gain, in dB: code 0.
    const MIN_DB: i32 = -35;

    /// The highest gain, in dB: code 31.
    const MAX_DB: i32 = -4;

    /// The gain for `db`, held to -35..=-4 dB: the YAML's scale, which libusdr's
    /// `_usdr_gain_clamp` applies before `lms6002d_set_txvga1_gain`.
    pub(super) fn for_db(db: i32) -> Self {
        let code = db.clamp(Self::MIN_DB, Self::MAX_DB) - Self::MIN_DB;
        let code = u8::try_from(code).expect("invariant: held to 0..=31");
        Self::new().with_code(u5::new(code))
    }
}

/// TXVGA2's gain, the RF level after the TX mixer, and the power detectors' multiplexer.
/// Datasheet `CTRL2`, register 0x45.
///
/// Bits 2:0 (`ENVD2`, `ENVD`) pick which envelope or peak detector the analog multiplexer
/// passes; libusdr writes them 0 with every gain (a `FIXME` in its source), as this type,
/// which leaves them out, does.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Vga2Gain {
    /// The gain in dB, 1 dB per step, 0 to 25; codes above 25 also give 25 dB.
    /// `VGA2GAIN`, bits 7:3.
    #[bits(3..=7)]
    db: u5,
}
impl Register for Vga2Gain {
    type Map = Reg;
    const ADDR: Reg = Reg::Vga2Gain;
}

impl Vga2Gain {
    /// The highest gain, in dB.
    const MAX_DB: i32 = 25;

    /// The gain for `db`, held to 0..=25 dB: the YAML's scale, which libusdr's
    /// `_usdr_gain_clamp` applies before `lms6002d_set_txvga2_gain`.
    pub(super) fn for_db(db: i32) -> Self {
        let db = u8::try_from(db.clamp(0, Self::MAX_DB)).expect("invariant: held to 0..=25");
        Self::new().with_db(u5::new(db))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn txvga1_codes_count_up_from_minus_35_db_and_hold_at_the_ends() {
        let code = |db| Vga1Gain::for_db(db).to_raw();
        assert_eq!(code(-35), 0);
        assert_eq!(code(-20), 15);
        assert_eq!(code(-4), 31);
        assert_eq!(code(i32::MIN), 0, "below the range: the lowest gain");
        assert_eq!(code(-3), 31, "above the range: the highest gain");
        assert_eq!(code(i32::MAX), 31);
    }

    #[test]
    fn txvga2_gain_sits_in_bits_7_to_3_and_holds_at_0_and_25_db() {
        // libusdr's `MAKE_LMS6002D_TRF_CTRL2(vga, 0, 0)` byte.
        let byte = |db| Vga2Gain::for_db(db).to_raw();
        assert_eq!(byte(0), 0);
        assert_eq!(byte(1), 0x08);
        assert_eq!(byte(25), 25 << 3);
        assert_eq!(byte(-1), 0, "below the range: 0 dB");
        assert_eq!(byte(26), 25 << 3, "above the range: 25 dB");
        assert_eq!(byte(i32::MIN), 0);
    }
}
