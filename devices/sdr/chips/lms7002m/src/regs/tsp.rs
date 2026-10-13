//! The transmit and receive digital signal processors (`TXTSP`, `RXTSP` pages): their mode
//! (test-signal generator, input, enable) and their stage bypasses.
//!
//! The pinned source names these fields and their options without describing them; names
//! follow the mnemonics.

use bnb::{BitEnum, bitfield, u1, u2};

use crate::reg::Reg;
use crate::register::Register;

/// The test-signal generator's full-scale level. `TSGFC`.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u1)]
pub enum TsgScale {
    /// −6 dB (`NEG6DB`, 0).
    Minus6Db,

    /// Full scale (`FS`, 1).
    FullScale,
}

/// The test-signal generator's mode. `TSGMODE`.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u1)]
pub enum TsgMode {
    /// From the NCO (`NCO`, 0).
    Nco,

    /// A DC level (`DC`, 1).
    Dc,
}

/// Where the processor takes its samples. `INSEL`.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u1)]
pub enum TspInput {
    /// The `LimeLight` interface (`LML`, 0).
    Lml,

    /// The test-signal generator (`TEST`, 1).
    Test,
}

/// What the receive processor captures. `CAPSEL`.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
pub enum Capture {
    /// RSSI (`RSSI`, 0).
    Rssi,

    /// ADC I and Q (`ADCI_ADCQ`, 1).
    AdcIq,

    /// `BSIGI_BSTATE`, 2.
    BsigIBstate,

    /// `BSIGQ_BSTATE`, 3.
    BsigQBstate,
}

/// Whether ADC captures take the processor's input or output. `CAPSEL_ADC`.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u1)]
pub enum CaptureAdc {
    /// The input (`RXTSP_INPUT`, 0).
    Input,

    /// The output (`RXTSP_OUTPUT`, 1).
    Output,
}

/// The complex mixer's direction. `CMIX_SC`.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u1)]
pub enum MixerDirection {
    /// Up-conversion (`UPCONVERT`, 0).
    Up,

    /// Down-conversion (`DOWNCONVERT`, 1).
    Down,
}

/// The transmit processor's mode and enable. `TXTSP_0x0200`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxTspMode {
    /// `TSGFC`, bit 9.
    #[bits(9..=9)]
    tsg_scale: TsgScale,

    /// The test signal's frequency: 1 = `DIV8`, 2 = `DIV4`. `TSGFCW`, bits 8:7.
    #[bits(7..=8)]
    tsg_frequency: u2,

    /// `TSGDCLDQ`, bit 6.
    #[bits(6..=6)]
    tsg_load_dc_q: bool,

    /// `TSGDCLDI`, bit 5.
    #[bits(5..=5)]
    tsg_load_dc_i: bool,

    /// `TSGSWAPIQ`, bit 4.
    #[bits(4..=4)]
    tsg_swap_iq: bool,

    /// `TSGMODE`, bit 3.
    #[bits(3..=3)]
    tsg_mode: TsgMode,

    /// `INSEL`, bit 2.
    #[bits(2..=2)]
    input: TspInput,

    /// `BSTART`, bit 1.
    #[bits(1..=1)]
    bstart: bool,

    /// The processor enabled. `EN`, bit 0.
    #[bits(0..=0)]
    enabled: bool,
}

impl Register for TxTspMode {
    const ADDR: Reg = Reg::TxTspMode;
}

/// The transmit processor's stage bypasses and mixer. `TXTSP_0x0208`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TxTspBypass {
    /// The complex mixer's gain: 0 = 0 dB, 1 = +6 dB, 2 = −6 dB. `CMIX_GAIN`, bits 15:14.
    #[bits(14..=15)]
    mixer_gain: u2,

    /// `CMIX_SC`, bit 13.
    #[bits(13..=13)]
    mixer_direction: MixerDirection,

    /// The complex mixer bypassed. `CMIX_BYP`, bit 8.
    #[bits(8..=8)]
    mixer_bypassed: bool,

    /// The inverse-sinc filter bypassed. `ISINC_BYP`, bit 7.
    #[bits(7..=7)]
    isinc_bypassed: bool,

    /// General FIR 3 bypassed. `GFIR3_BYP`, bit 6.
    #[bits(6..=6)]
    gfir3_bypassed: bool,

    /// General FIR 2 bypassed. `GFIR2_BYP`, bit 5.
    #[bits(5..=5)]
    gfir2_bypassed: bool,

    /// General FIR 1 bypassed. `GFIR1_BYP`, bit 4.
    #[bits(4..=4)]
    gfir1_bypassed: bool,

    /// DC correction bypassed. `DC_BYP`, bit 3.
    #[bits(3..=3)]
    dc_bypassed: bool,

    /// Gain correction bypassed. `GC_BYP`, bit 1.
    #[bits(1..=1)]
    gain_bypassed: bool,

    /// Phase correction bypassed. `PH_BYP`, bit 0.
    #[bits(0..=0)]
    phase_bypassed: bool,
}

impl Register for TxTspBypass {
    const ADDR: Reg = Reg::TxTspBypass;
}

/// The receive processor's capture, mode and enable. `RXTSP_0x0400`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RxTspMode {
    /// `CAPTURE`, bit 15.
    #[bits(15..=15)]
    capture: bool,

    /// `CAPSEL`, bits 14:13.
    #[bits(13..=14)]
    capture_select: Capture,

    /// `CAPSEL_ADC`, bit 12.
    #[bits(12..=12)]
    capture_adc: CaptureAdc,

    /// `TSGFC`, bit 9.
    #[bits(9..=9)]
    tsg_scale: TsgScale,

    /// The test signal's frequency: 1 = `DIV8`, 2 = `DIV4`. `TSGFCW`, bits 8:7.
    #[bits(7..=8)]
    tsg_frequency: u2,

    /// `TSGDCLDQ`, bit 6.
    #[bits(6..=6)]
    tsg_load_dc_q: bool,

    /// `TSGDCLDI`, bit 5.
    #[bits(5..=5)]
    tsg_load_dc_i: bool,

    /// `TSGSWAPIQ`, bit 4.
    #[bits(4..=4)]
    tsg_swap_iq: bool,

    /// `TSGMODE`, bit 3.
    #[bits(3..=3)]
    tsg_mode: TsgMode,

    /// `INSEL`, bit 2.
    #[bits(2..=2)]
    input: TspInput,

    /// `BSTART`, bit 1.
    #[bits(1..=1)]
    bstart: bool,

    /// The processor enabled. `EN`, bit 0.
    #[bits(0..=0)]
    enabled: bool,
}

impl Register for RxTspMode {
    const ADDR: Reg = Reg::RxTspMode;
}

/// The receive processor's stage bypasses and mixer. `RXTSP_0x040C`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RxTspBypass {
    /// The complex mixer's gain: 0 = 0 dB, 1 = +6 dB, 2 = −6 dB. `CMIX_GAIN`, bits 15:14.
    #[bits(14..=15)]
    mixer_gain: u2,

    /// `CMIX_SC`, bit 13.
    #[bits(13..=13)]
    mixer_direction: MixerDirection,

    /// `CMIX_GAINM`, bit 12.
    #[bits(12..=12)]
    mixer_gain_m: bool,

    /// `DCLOOP_STOP`, bit 8.
    #[bits(8..=8)]
    dc_loop_stopped: bool,

    /// The complex mixer bypassed. `CMIX_BYP`, bit 7.
    #[bits(7..=7)]
    mixer_bypassed: bool,

    /// The AGC bypassed. `AGC_BYP`, bit 6.
    #[bits(6..=6)]
    agc_bypassed: bool,

    /// General FIR 3 bypassed. `GFIR3_BYP`, bit 5.
    #[bits(5..=5)]
    gfir3_bypassed: bool,

    /// General FIR 2 bypassed. `GFIR2_BYP`, bit 4.
    #[bits(4..=4)]
    gfir2_bypassed: bool,

    /// General FIR 1 bypassed. `GFIR1_BYP`, bit 3.
    #[bits(3..=3)]
    gfir1_bypassed: bool,

    /// DC correction bypassed. `DC_BYP`, bit 2.
    #[bits(2..=2)]
    dc_bypassed: bool,

    /// Gain correction bypassed. `GC_BYP`, bit 1.
    #[bits(1..=1)]
    gain_bypassed: bool,

    /// Phase correction bypassed. `PH_BYP`, bit 0.
    #[bits(0..=0)]
    phase_bypassed: bool,
}

impl Register for RxTspBypass {
    const ADDR: Reg = Reg::RxTspBypass;
}
