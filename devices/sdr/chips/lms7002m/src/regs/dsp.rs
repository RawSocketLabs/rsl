//! The processors' decimation and interpolation, DC corrector, NCOs and built-in self-test
//! status (`RXTSP`, `TXTSP`, `RXNCO`, `TXNCO` pages), beside the mode and bypass registers
//! in `tsp.rs`.
//!
//! The pinned source names these fields without describing them; names follow the
//! mnemonics.

use bnb::{bitfield, u3, u4, u12};

use crate::reg::Reg;
use crate::register::Register;

/// The receive half-band decimation and the phase correction. `RXTSP_0x0403`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RxTspDecimation {
    /// Decimation by 2^(this + 1); 7 bypasses it. `HBD_OVR`, bits 14:12.
    #[bits(12..=14)]
    order: u3,

    /// The I/Q phase correction. `IQCORR`, bits 11:0.
    #[bits(0..=11)]
    phase_correction: u12,
}

impl Register for RxTspDecimation {
    const ADDR: Reg = Reg::RxTspDecimation;
}

/// The transmit half-band interpolation and the phase correction. `TXTSP_0x0203`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxTspInterpolation {
    /// Interpolation by 2^(this + 1); 7 bypasses it. `HBI_OVR`, bits 14:12.
    #[bits(12..=14)]
    order: u3,

    /// The I/Q phase correction. `IQCORR`, bits 11:0.
    #[bits(0..=11)]
    phase_correction: u12,
}

impl Register for TxTspInterpolation {
    const ADDR: Reg = Reg::TxTspInterpolation;
}

/// The receive DC corrector's averaging. `RXTSP_0x0404`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RxTspDc {
    /// `HBD_DLY`, bits 15:13.
    #[bits(13..=15)]
    decimator_delay: u3,

    /// The averaging window code. `DCCORR_AVG`, bits 2:0.
    #[bits(0..=2)]
    window: u3,
}

impl Register for RxTspDc {
    const ADDR: Reg = Reg::RxTspDc;
}

/// An NCO's mode: `RXNCO_0x0440` and `TXNCO_0x0240`, which share the layout. libusdr
/// writes it at [`Reg::RxNcoMode`] or [`Reg::TxNcoMode`]; the frequency control word's
/// halves follow at `FCW0_HI` (0x0442, 0x0242) and `FCW0_LO` (0x0443, 0x0243) as plain
/// 16-bit values.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NcoMode {
    /// `DTHBIT`, bits 8:5; libusdr writes 1.
    #[bits(5..=8)]
    dither: u4,

    /// The frequency or phase word in use. `SEL`, bits 4:1.
    #[bits(1..=4)]
    select: u4,

    /// Phase words rather than frequency words (`MODE`: `FCW` 0, `PHO` 1), bit 0.
    #[bits(0..=0)]
    phase_mode: bool,
}

/// A processor's built-in self-test status as read: `RXTSP` `0x040E` (after selecting
/// `BSIGI_BSTATE` capture) or `TXTSP` `0x0209`. libusdr reads only bit 0.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BistStatus {
    /// The self-test still runs: `lms7002m_xxtsp_bst_isdone` reports done when it is clear.
    #[bits(0..=0)]
    running: bool,
}
