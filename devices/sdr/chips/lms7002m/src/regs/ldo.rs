//! The internal LDO regulators (`LDO` page): enables, bypasses, load-impedance enables and
//! the digital-interface LDOs.
//!
//! Each LDO is named for the block it feeds; the pinned source gives the mnemonics only
//! (`EN_LDO_<block>`, `BYP_LDO_<block>`, `EN_LOADIMP_LDO_<block>`), so the fields keep the
//! block names and say which mnemonic each is.

use bnb::bitfield;

use crate::reg::Reg;
use crate::register::Register;

/// LDO enables, first part, and the LDOs' global enable. `LDO_0x0092`; `lms7002m_create`
/// writes the low half of the board's LDO mask here.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LdoEnables {
    /// `EN_LDO_DIG`, bit 15.
    #[bits(15..=15)]
    dig: bool,
    /// `EN_LDO_DIGGN`, bit 14.
    #[bits(14..=14)]
    dig_gn: bool,
    /// `EN_LDO_DIGSXR`, bit 13.
    #[bits(13..=13)]
    dig_sxr: bool,
    /// `EN_LDO_DIGSXT`, bit 12.
    #[bits(12..=12)]
    dig_sxt: bool,
    /// `EN_LDO_DIVGN`, bit 11.
    #[bits(11..=11)]
    div_gn: bool,
    /// `EN_LDO_DIVSXR`, bit 10.
    #[bits(10..=10)]
    div_sxr: bool,
    /// `EN_LDO_DIVSXT`, bit 9.
    #[bits(9..=9)]
    div_sxt: bool,
    /// `EN_LDO_LNA12`, bit 8.
    #[bits(8..=8)]
    lna12: bool,
    /// `EN_LDO_LNA14`, bit 7.
    #[bits(7..=7)]
    lna14: bool,
    /// `EN_LDO_MXRFE`, bit 6.
    #[bits(6..=6)]
    mx_rfe: bool,
    /// `EN_LDO_RBB`, bit 5.
    #[bits(5..=5)]
    rbb: bool,
    /// `EN_LDO_RXBUF`, bit 4.
    #[bits(4..=4)]
    rx_buf: bool,
    /// `EN_LDO_TBB`, bit 3.
    #[bits(3..=3)]
    tbb: bool,
    /// `EN_LDO_TIA12`, bit 2.
    #[bits(2..=2)]
    tia12: bool,
    /// `EN_LDO_TIA14`, bit 1.
    #[bits(1..=1)]
    tia14: bool,
    /// The LDOs' global enable. `EN_G_LDO`, bit 0.
    #[bits(0..=0)]
    enabled: bool,
}

impl Register for LdoEnables {
    const ADDR: Reg = Reg::LdoEnables;
}

/// Load-impedance enables, first part, and LDO enables, second part. `LDO_0x0093`;
/// `lms7002m_create` writes the high half of the board's LDO mask here.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LdoEnablesLoad {
    /// `EN_LOADIMP_LDO_TLOB`, bit 15.
    #[bits(15..=15)]
    load_tlob: bool,
    /// `EN_LOADIMP_LDO_TPAD`, bit 14.
    #[bits(14..=14)]
    load_tpad: bool,
    /// `EN_LOADIMP_LDO_TXBUF`, bit 13.
    #[bits(13..=13)]
    load_tx_buf: bool,
    /// `EN_LOADIMP_LDO_VCOGN`, bit 12.
    #[bits(12..=12)]
    load_vco_gn: bool,
    /// `EN_LOADIMP_LDO_VCOSXR`, bit 11.
    #[bits(11..=11)]
    load_vco_sxr: bool,
    /// `EN_LOADIMP_LDO_VCOSXT`, bit 10.
    #[bits(10..=10)]
    load_vco_sxt: bool,
    /// `EN_LDO_AFE`, bit 9.
    #[bits(9..=9)]
    afe: bool,
    /// `EN_LDO_CPGN`, bit 8.
    #[bits(8..=8)]
    cp_gn: bool,
    /// `EN_LDO_CPSXR`, bit 7.
    #[bits(7..=7)]
    cp_sxr: bool,
    /// `EN_LDO_TLOB`, bit 6.
    #[bits(6..=6)]
    tlob: bool,
    /// `EN_LDO_TPAD`, bit 5.
    #[bits(5..=5)]
    tpad: bool,
    /// `EN_LDO_TXBUF`, bit 4.
    #[bits(4..=4)]
    tx_buf: bool,
    /// `EN_LDO_VCOGN`, bit 3.
    #[bits(3..=3)]
    vco_gn: bool,
    /// `EN_LDO_VCOSXR`, bit 2.
    #[bits(2..=2)]
    vco_sxr: bool,
    /// `EN_LDO_VCOSXT`, bit 1.
    #[bits(1..=1)]
    vco_sxt: bool,
    /// `EN_LDO_CPSXT`, bit 0.
    #[bits(0..=0)]
    cp_sxt: bool,
}

impl Register for LdoEnablesLoad {
    const ADDR: Reg = Reg::LdoEnablesLoad;
}

/// LDO bypasses, first part, and load-impedance enables, second part. `LDO_0x0095`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LdoBypassLoad {
    /// `BYP_LDO_TBB`, bit 15.
    #[bits(15..=15)]
    bypass_tbb: bool,
    /// `BYP_LDO_TIA12`, bit 14.
    #[bits(14..=14)]
    bypass_tia12: bool,
    /// `BYP_LDO_TIA14`, bit 13.
    #[bits(13..=13)]
    bypass_tia14: bool,
    /// `BYP_LDO_TLOB`, bit 12.
    #[bits(12..=12)]
    bypass_tlob: bool,
    /// `BYP_LDO_TPAD`, bit 11.
    #[bits(11..=11)]
    bypass_tpad: bool,
    /// `BYP_LDO_TXBUF`, bit 10.
    #[bits(10..=10)]
    bypass_tx_buf: bool,
    /// `BYP_LDO_VCOGN`, bit 9.
    #[bits(9..=9)]
    bypass_vco_gn: bool,
    /// `BYP_LDO_VCOSXR`, bit 8.
    #[bits(8..=8)]
    bypass_vco_sxr: bool,
    /// `BYP_LDO_VCOSXT`, bit 7.
    #[bits(7..=7)]
    bypass_vco_sxt: bool,
    /// `EN_LOADIMP_LDO_AFE`, bit 2.
    #[bits(2..=2)]
    load_afe: bool,
    /// `EN_LOADIMP_LDO_CPGN`, bit 1.
    #[bits(1..=1)]
    load_cp_gn: bool,
    /// `EN_LOADIMP_LDO_CPSXR`, bit 0.
    #[bits(0..=0)]
    load_cp_sxr: bool,
}

impl Register for LdoBypassLoad {
    const ADDR: Reg = Reg::LdoBypassLoad;
}

/// LDO bypasses, second part. `LDO_0x0096`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LdoBypass {
    /// `BYP_LDO_AFE`, bit 15.
    #[bits(15..=15)]
    afe: bool,
    /// `BYP_LDO_CPGN`, bit 14.
    #[bits(14..=14)]
    cp_gn: bool,
    /// `BYP_LDO_CPSXR`, bit 13.
    #[bits(13..=13)]
    cp_sxr: bool,
    /// `BYP_LDO_CPSXT`, bit 12.
    #[bits(12..=12)]
    cp_sxt: bool,
    /// `BYP_LDO_DIG`, bit 11.
    #[bits(11..=11)]
    dig: bool,
    /// `BYP_LDO_DIGGN`, bit 10.
    #[bits(10..=10)]
    dig_gn: bool,
    /// `BYP_LDO_DIGSXR`, bit 9.
    #[bits(9..=9)]
    dig_sxr: bool,
    /// `BYP_LDO_DIGSXT`, bit 8.
    #[bits(8..=8)]
    dig_sxt: bool,
    /// `BYP_LDO_DIVGN`, bit 7.
    #[bits(7..=7)]
    div_gn: bool,
    /// `BYP_LDO_DIVSXR`, bit 6.
    #[bits(6..=6)]
    div_sxr: bool,
    /// `BYP_LDO_DIVSXT`, bit 5.
    #[bits(5..=5)]
    div_sxt: bool,
    /// `BYP_LDO_LNA12`, bit 4.
    #[bits(4..=4)]
    lna12: bool,
    /// `BYP_LDO_LNA14`, bit 3.
    #[bits(3..=3)]
    lna14: bool,
    /// `BYP_LDO_MXRFE`, bit 2.
    #[bits(2..=2)]
    mx_rfe: bool,
    /// `BYP_LDO_RBB`, bit 1.
    #[bits(1..=1)]
    rbb: bool,
    /// `BYP_LDO_RXBUF`, bit 0.
    #[bits(0..=0)]
    rx_buf: bool,
}

impl Register for LdoBypass {
    const ADDR: Reg = Reg::LdoBypass;
}

/// The SPI-buffer and digital-interface LDOs (`SPIBUF`, `DIGIp2`, `DIGIp1`): speed-up,
/// bypass, load impedance and power-down for each, and their global enable. `LDO_0x00A6`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LdoDigital {
    /// `SPDUP_LDO_SPIBUF`, bit 12.
    #[bits(12..=12)]
    speed_up_spi_buf: bool,
    /// `SPDUP_LDO_DIGIp2`, bit 11.
    #[bits(11..=11)]
    speed_up_digi_p2: bool,
    /// `SPDUP_LDO_DIGIp1`, bit 10.
    #[bits(10..=10)]
    speed_up_digi_p1: bool,
    /// `BYP_LDO_SPIBUF`, bit 9.
    #[bits(9..=9)]
    bypass_spi_buf: bool,
    /// `BYP_LDO_DIGIp2`, bit 8.
    #[bits(8..=8)]
    bypass_digi_p2: bool,
    /// `BYP_LDO_DIGIp1`, bit 7.
    #[bits(7..=7)]
    bypass_digi_p1: bool,
    /// `EN_LOADIMP_LDO_SPIBUF`, bit 6.
    #[bits(6..=6)]
    load_spi_buf: bool,
    /// `EN_LOADIMP_LDO_DIGIp2`, bit 5.
    #[bits(5..=5)]
    load_digi_p2: bool,
    /// `EN_LOADIMP_LDO_DIGIp1`, bit 4.
    #[bits(4..=4)]
    load_digi_p1: bool,
    /// The SPI-buffer LDO powered down. `PD_LDO_SPIBUF`, bit 3.
    #[bits(3..=3)]
    spi_buf_powered_down: bool,
    /// The `DIGIp2` LDO powered down. `PD_LDO_DIGIp2`, bit 2.
    #[bits(2..=2)]
    digi_p2_powered_down: bool,
    /// The `DIGIp1` LDO powered down. `PD_LDO_DIGIp1`, bit 1.
    #[bits(1..=1)]
    digi_p1_powered_down: bool,
    /// Their global enable. `EN_G_LDOP`, bit 0.
    #[bits(0..=0)]
    enabled: bool,
}

impl Register for LdoDigital {
    const ADDR: Reg = Reg::LdoDigital;
}
