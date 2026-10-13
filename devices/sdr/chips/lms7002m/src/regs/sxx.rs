//! The synthesizer page's per-channel `EN_DIR` bits (`SXX` page).

use bnb::bitfield;

use crate::reg::Reg;
use crate::register::Register;

/// One `EN_DIR` bit per block. The pinned source names them only; libusdr sets the RFE bit
/// while a channel's receive front end is enabled, and the TRF bit from the same RFE
/// enables (`lms7002m_trf_path` reads `rfe[i].en`, not `trf[i].en`). `SXX_0x0124`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EnDir {
    /// `EN_DIR_SXX`, bit 4.
    #[bits(4..=4)]
    sxx: bool,

    /// `EN_DIR_RBB`, bit 3.
    #[bits(3..=3)]
    rbb: bool,

    /// `EN_DIR_RFE`, bit 2.
    #[bits(2..=2)]
    rfe: bool,

    /// `EN_DIR_TBB`, bit 1.
    #[bits(1..=1)]
    tbb: bool,

    /// `EN_DIR_TRF`, bit 0.
    #[bits(0..=0)]
    trf: bool,
}

impl Register for EnDir {
    const ADDR: Reg = Reg::EnDir;
}
