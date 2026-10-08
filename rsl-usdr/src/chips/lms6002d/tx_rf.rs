//! The TX RF section (datasheet `TRF`, registers 0x40-0x4F): the power amplifiers and
//! their drive.

use bnb::{BitEnum, bitfield, u2, u4};

use super::spi::BlockReg;
use crate::chips::register::Register;

/// TX RF register addresses.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
pub(super) enum Reg {
    /// Datasheet `PA_CTRL`; see [`PaSelect`].
    PaSelect = 0x44,
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
    /// PA1.
    Pa1,
    /// PA2.
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
