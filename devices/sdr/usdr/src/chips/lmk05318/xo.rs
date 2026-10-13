//! The XO input: its buffer, doubler and divider towards APLL1, and the BAW lock-detection
//! constants libusdr loads per XO frequency.

use bnb::{BitEnum, bitfield, u3, u4, u5};

use super::plan::{Xo, XoInput, XoPath};
use super::reg::{Lmk05318Register, Reg};

/// The XO path's controls. `XO_CLKCTL1`, 0x2A.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct XoControl {
    /// The XO doubler is on. `OSCIN_DBLR_EN`, bit 4.
    #[bits(4..=4)]
    doubler: bool,

    /// Ignore the XO frequency detector: the XO counts as valid. `XO_FDET_BYP`, bit 3.
    #[bits(3..=3)]
    frequency_detector_bypass: bool,

    /// Ignore the XO amplitude detector. `XO_DETECT_BYP`, bit 2.
    #[bits(2..=2)]
    amplitude_detector_bypass: bool,

    /// The XO input buffer is on. `XO_BUFSEL`, bit 0.
    #[bits(0..=0)]
    buffer_on: bool,
}
impl Lmk05318Register for XoControl {
    const ADDR: Reg = Reg::XoControl;
}

impl XoControl {
    /// The controls for `xo` reaching APLL1 through `path`, buffer on, amplitude detector
    /// active (`lmk05318_set_xo_fref`).
    pub(super) fn for_path(xo: Xo, path: XoPath) -> Self {
        Self::new()
            .with_doubler(path.doubler)
            .with_frequency_detector_bypass(xo.detector_bypass)
            .with_buffer_on(true)
    }
}

/// The XO input buffer type (`XO_TYPE`; the YAML's `x-in-opts`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u4)]
#[repr(u8)]
pub(super) enum XoTypeCode {
    /// DC-coupled differential, external termination (`DC_DIFF_EXT`).
    DcDifferentialExternal = 0x0,

    /// AC-coupled differential, external termination (`AC_DIFF_EXT`).
    AcDifferentialExternal = 0x1,

    /// AC-coupled differential, internal 100 Ω termination (`AC_DIFF_INT_100`).
    AcDifferentialInternal100 = 0x3,

    /// HCSL, internal 50 Ω termination (`HCSL_INT_50`).
    HcslInternal50 = 0x4,

    /// LVCMOS (`CMOS`).
    Cmos = 0x8,

    /// Single-ended, internal 50 Ω termination (`SE_INT_50`).
    SingleEndedInternal50 = 0xc,

    /// Codes the YAML does not list.
    #[catch_all]
    Other(u4),
}

impl From<XoInput> for XoTypeCode {
    fn from(input: XoInput) -> Self {
        match input {
            XoInput::DcDifferentialExternal => Self::DcDifferentialExternal,
            XoInput::AcDifferentialExternal => Self::AcDifferentialExternal,
            XoInput::AcDifferentialInternal100 => Self::AcDifferentialInternal100,
            XoInput::HcslInternal50 => Self::HcslInternal50,
            XoInput::Cmos => Self::Cmos,
            XoInput::SingleEndedInternal50 => Self::SingleEndedInternal50,
        }
    }
}

/// The XO input type and two reserved fields. `XO_CLKCTL2`, 0x2B.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct XoInputType {
    /// Reserved; the YAML gives its reset as 1 and libusdr writes 1.
    /// `XO_CLKCTL2_RESERVED7`, bit 7.
    #[bits(7..=7)]
    reserved_7: bool,

    /// The XO input buffer type. `XO_TYPE`, bits 6:3.
    #[bits(3..=6)]
    input: XoTypeCode,

    /// Reserved; libusdr writes 2 (the YAML's reset of 0x10 does not fit three bits).
    /// `XO_CLKCTL2_RESERVED0`, bits 2:0.
    #[bits(0..=2)]
    reserved_2_0: u3,
}
impl Lmk05318Register for XoInputType {
    const ADDR: Reg = Reg::XoInputType;
}

impl XoInputType {
    /// The value libusdr writes for `input`: its type, reserved fields 1 and 2.
    pub(super) fn for_input(input: XoInput) -> Self {
        Self::new()
            .with_reserved_7(true)
            .with_input(input.into())
            .with_reserved_2_0(u3::new(2))
    }
}

/// The XO's divider towards APLL1. `XO_CONFIG`, 0x2C.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct XoDivider {
    /// The divide ratio less one, 0 to 31. `OSCIN_RDIV`, bits 4:0.
    #[bits(0..=4)]
    ratio_minus_one: u5,
}
impl Lmk05318Register for XoDivider {
    const ADDR: Reg = Reg::XoDivider;
}

impl XoDivider {
    /// The register for `path`'s divider, masked to five bits as the C's macro masks it.
    pub(super) fn for_path(path: XoPath) -> Self {
        let ratio = path.divider.wrapping_sub(1).to_le_bytes()[0];
        Self::new().with_ratio_minus_one(u5::from_raw(ratio))
    }
}

/// BAW lock and unlock detection for a 12.8 MHz XO, 0x51 to 0x63
/// (`lmk05318_set_xo_bawdetect_registers`, `XO12_8`): tuning constants.
const BAW_DETECTION_12_8: [(Reg, u8); 19] = baw_detection([0x07, 0x80]);
/// The same for 25 MHz (`XO25`).
const BAW_DETECTION_25: [(Reg, u8); 19] = baw_detection([0x0e, 0xa6]);
/// The same for 26 and 52 MHz, which both give APLL1 a 52 MHz phase detector (`XO26`,
/// `XO52`).
const BAW_DETECTION_26_52: [(Reg, u8); 19] = baw_detection([0x0f, 0x3c]);

/// libusdr's BAW detection table, which differs between XOs only in the lock and unlock
/// counter starts' bits 23:8 (0x53-0x54 and 0x5D-0x5E), equal to each other in every
/// table: `count`.
const fn baw_detection(count: [u8; 2]) -> [(Reg, u8); 19] {
    [
        (Reg::BawLockPpmMaxLow, 0x0a),
        (Reg::BawLockCountStart3, 0x00),
        (Reg::BawLockCountStart2, count[0]),
        (Reg::BawLockCountStart1, count[1]),
        (Reg::BawLockCountStart0, 0x00),
        (Reg::BawLockVcoCountStart3, 0x00),
        (Reg::BawLockVcoCountStart2, 0x1e),
        (Reg::BawLockVcoCountStart1, 0x84),
        (Reg::BawLockVcoCountStart0, 0x80),
        (Reg::BawUnlockPpmMaxHigh, 0x00),
        (Reg::BawUnlockPpmMaxLow, 0x14),
        (Reg::BawUnlockCountStart3, 0x00),
        (Reg::BawUnlockCountStart2, count[0]),
        (Reg::BawUnlockCountStart1, count[1]),
        (Reg::BawUnlockCountStart0, 0x00),
        (Reg::BawUnlockVcoCountStart3, 0x00),
        (Reg::BawUnlockVcoCountStart2, 0x1e),
        (Reg::BawUnlockVcoCountStart1, 0x84),
        (Reg::BawUnlockVcoCountStart0, 0x80),
    ]
}

/// libusdr's BAW lock-detection constants for an `xo_hz` XO; `None` for XOs it has none
/// for (only 12.8, 25, 26 and 52 MHz).
pub(super) const fn baw_detection_for(xo_hz: u32) -> Option<&'static [(Reg, u8); 19]> {
    match xo_hz {
        12_800_000 => Some(&BAW_DETECTION_12_8),
        25_000_000 => Some(&BAW_DETECTION_25),
        26_000_000 | 52_000_000 => Some(&BAW_DETECTION_26_52),
        _ => None,
    }
}
