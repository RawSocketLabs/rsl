//! The analog DC-offset correction DACs and their calibration comparators (`DC` page): the
//! power-downs, the calibration control, each DAC's value and the clock dividers.
//!
//! The pinned source names these fields without describing them; names follow the
//! mnemonics.

use bnb::{bitfield, u11};

use crate::reg::Reg;
use crate::register::Register;

/// The DC correction's mode and the DACs' and comparators' power-downs. `DC_PD`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DcPowerDown {
    /// `DCMODE`, bit 15: `lms7002m_dc_corr_en` sets it while any DAC runs.
    #[bits(15..=15)]
    mode: bool,

    /// `PD_DCDAC_RXB`, bit 7.
    #[bits(7..=7)]
    dac_rx_b_powered_down: bool,

    /// `PD_DCDAC_RXA`, bit 6.
    #[bits(6..=6)]
    dac_rx_a_powered_down: bool,

    /// `PD_DCDAC_TXB`, bit 5.
    #[bits(5..=5)]
    dac_tx_b_powered_down: bool,

    /// `PD_DCDAC_TXA`, bit 4.
    #[bits(4..=4)]
    dac_tx_a_powered_down: bool,

    /// `PD_DCCMP_RXB`, bit 3.
    #[bits(3..=3)]
    comparator_rx_b_powered_down: bool,

    /// `PD_DCCMP_RXA`, bit 2.
    #[bits(2..=2)]
    comparator_rx_a_powered_down: bool,

    /// `PD_DCCMP_TXB`, bit 1.
    #[bits(1..=1)]
    comparator_tx_b_powered_down: bool,

    /// `PD_DCCMP_TXA`, bit 0.
    #[bits(0..=0)]
    comparator_tx_a_powered_down: bool,
}

impl Register for DcPowerDown {
    const ADDR: Reg = Reg::DcPowerDown;
}

/// The DC calibration's comparator configuration and start bits. `DC_0x05C2`.
///
/// Each byte holds one bit per DAC, from bit 7 down: RX B Q, RX B I, RX A Q, RX A I, TX B Q,
/// TX B I, TX A Q, TX A I (`DCCAL_CMPCFG_*` in the high byte, `DCCAL_START_*` in the low).
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DcCalibration {
    /// `DCCAL_CMPCFG_*`, bits 15:8.
    #[bits(8..=15)]
    comparator_config: u8,

    /// `DCCAL_START_*`, bits 7:0.
    #[bits(0..=7)]
    start: u8,
}

impl Register for DcCalibration {
    const ADDR: Reg = Reg::DcCalibration;
}

/// One DC correction DAC's word: `DC_0x05C3` to `DC_0x05CA`, which share the layout; write it
/// at the DAC's [`DcDac::reg`](crate::DcDac::reg).
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DcDacWord {
    /// `DCWR_*`, bit 15: `lms7002m_dc_corr` writes the value with it clear, then set.
    #[bits(15..=15)]
    write: bool,

    /// `DCRD_*`, bit 14.
    #[bits(14..=14)]
    read: bool,

    /// The DAC's value, an 11-bit code. `DC_*`, bits 10:0.
    #[bits(0..=10)]
    value: u11,
}

/// The DC calibration's clock dividers. `DC_0x05CB`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DcClockDivider {
    /// `DC_RXCDIV`, bits 15:8.
    #[bits(8..=15)]
    rx: u8,

    /// `DC_TXCDIV`, bits 7:0.
    #[bits(0..=7)]
    tx: u8,
}

impl Register for DcClockDivider {
    const ADDR: Reg = Reg::DcClockDivider;
}
