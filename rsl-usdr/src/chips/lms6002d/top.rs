//! The top-level block (datasheet `Top`, registers 0x00-0x0F): chip identity, which
//! blocks are powered, clock gating, and the reference-clock buffer.

use bnb::{BitEnum, bitfield, u4};

use super::spi::BlockReg;
use crate::chips::register::Register;

/// Top-level register addresses.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
pub(super) enum Reg {
    /// Datasheet `CHIPID`; see [`ChipId`].
    ChipId = 0x04,
    /// Datasheet `ENCFG`; see [`EnableConfig`].
    EnableConfig = 0x05,
    /// Datasheet `ENREG`; see [`ClockEnables`].
    ClockEnables = 0x09,
    /// Datasheet `POWER`; see [`ReferencePower`].
    ReferencePower = 0x0b,
}

impl BlockReg for Reg {}

/// The chip's version and revision. Datasheet `CHIPID`, register 0x04 (read-only).
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ChipId {
    /// Chip version. `VER`, bits 7:4.
    #[bits(4..=7)]
    version: u4,
    /// Chip revision. `REV`, bits 3:0.
    #[bits(0..=3)]
    revision: u4,
}
impl Register for ChipId {
    type Map = Reg;
    const ADDR: Reg = Reg::ChipId;
}

/// Which top-level blocks are powered, and how the SPI port is wired. Datasheet `ENCFG`,
/// register 0x05.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct EnableConfig {
    /// Take block control signals from the test-mode registers instead of decoding them.
    /// `DECODE`, bit 7; reset 0.
    #[bits(7..=7)]
    test_mode_controls: bool,
    /// Run the delta-sigma modulators; clear holds them in soft reset. `SRESET`, bit 5
    /// (active-low reset); reset 1.
    #[bits(5..=5)]
    modulators_running: bool,
    /// Power the top-level modules. `EN`, bit 4; reset 1.
    #[bits(4..=4)]
    top_enabled: bool,
    /// Power the transmitter. `STXEN`, bit 3; reset 0.
    #[bits(3..=3)]
    tx_enabled: bool,
    /// Power the receiver. `SRXEN`, bit 2; reset 0.
    #[bits(2..=2)]
    rx_enabled: bool,
    /// Use the four-wire SPI port instead of three-wire. `TFWMODE`, bit 1; reset 1.
    #[bits(1..=1)]
    four_wire_spi: bool,
}
impl Register for EnableConfig {
    type Map = Reg;
    const ADDR: Reg = Reg::EnableConfig;
}

/// Clock gates for the PLL modulators' SPI and the calibration blocks, and the RX output
/// pin switch. Datasheet `ENREG`, register 0x09.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ClockEnables {
    /// Connect the RX output to the ADC input pins; power RXVGA2 down first. `RXOUTSW`,
    /// bit 7; reset 0 (pins disconnected).
    #[bits(7..=7)]
    rx_output_to_adc: bool,
    /// Drive the PLL clock output pin. `CLK_PLLCLKOUT`, bit 6.
    #[bits(6..=6)]
    pll_clock_out: bool,
    /// Clock the LPF tuning calibration. `CLK_LPF_CAL`, bit 5.
    #[bits(5..=5)]
    lpf_calibration_clock: bool,
    /// Clock the RXVGA2 DC-offset calibration. `CLK_RX_VGA2_DCCAL`, bit 4.
    #[bits(4..=4)]
    rx_vga2_dc_calibration_clock: bool,
    /// Clock the RX LPF DC-offset calibration. `CLK_RX_RPL_DCCAL`, bit 3.
    #[bits(3..=3)]
    rx_lpf_dc_calibration_clock: bool,
    /// Clock the RX PLL modulator's SPI. `CLK_RX_DSM_SPI`, bit 2.
    #[bits(2..=2)]
    rx_pll_modulator_clock: bool,
    /// Clock the TX LPF DC-offset calibration. `CLK_TX_RPL_DCCAL`, bit 1.
    #[bits(1..=1)]
    tx_lpf_dc_calibration_clock: bool,
    /// Clock the TX PLL modulator's SPI. `CLK_TX_DSM_SPI`, bit 0.
    #[bits(0..=0)]
    tx_pll_modulator_clock: bool,
}
impl Register for ClockEnables {
    type Map = Reg;
    const ADDR: Reg = Reg::ClockEnables;
}

/// The reference-clock (XCO) input buffer, the LPF calibration reference, and the RF
/// loopback switch. Datasheet `POWER`, register 0x0B.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ReferencePower {
    /// Power the reference-clock buffer down. `PDXCOBUF`, bit 4; reset 0.
    #[bits(4..=4)]
    xco_buffer_off: bool,
    /// Self-bias the reference-clock buffer. `SLFBXCOBUF`, bit 3; reset 1.
    #[bits(3..=3)]
    xco_buffer_self_biased: bool,
    /// Bypass the reference-clock buffer. `BYPXCOBUF`, bit 2; reset 0.
    #[bits(2..=2)]
    xco_buffer_bypassed: bool,
    /// Power the LPF calibration's DC reference down. `PD_DCOREF_LPFCAL`, bit 1.
    #[bits(1..=1)]
    lpf_calibration_reference_off: bool,
    /// Power the RF loopback switch up. `PU_RFLB`, bit 0.
    #[bits(0..=0)]
    rf_loopback_on: bool,
}
impl Register for ReferencePower {
    type Map = Reg;
    const ADDR: Reg = Reg::ReferencePower;
}
