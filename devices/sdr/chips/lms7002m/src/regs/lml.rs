//! The `LimeLight` digital interface's controls (`LML` page): resets, channel enables, the
//! channel selection for banked registers, MIMO, and the chip version.

use bnb::{BitEnum, bitfield, u2, u5, u6};

use crate::reg::Reg;
use crate::register::Register;

/// Which channel's copy of a banked register an access reaches (`MAC`): libusdr's
/// `LMS7_CH_*` and the YAML's `MAC` options.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
pub enum Mac {
    /// Neither channel (`NONE`, 0).
    None,

    /// Channel A (`CHA`, 1).
    A,

    /// Channel B (`CHB`, 2).
    B,

    /// Both: writes reach both copies (`CHAB`, 3).
    Both,
}

/// `LimeLight` resets and enables, and the bank selection. `LML_0x0020`.
///
/// The pinned source names the ten reset bits (`LRST_*`, `MRST_*`, `SRST_*FIFO`) without
/// their polarity or reset values, so they keep their mnemonics: `lms7002m_create` writes
/// them all clear and then all set, and keeps them set; `lms7002m_limelight_reset`, which
/// libusdr comments "Reset LML FIFO", writes them set and then restores its cached value.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LmlControl {
    /// `LRST_TX_B`, bit 15.
    #[bits(15..=15)]
    lrst_tx_b: bool,

    /// `MRST_TX_B`, bit 14.
    #[bits(14..=14)]
    mrst_tx_b: bool,

    /// `LRST_TX_A`, bit 13.
    #[bits(13..=13)]
    lrst_tx_a: bool,

    /// `MRST_TX_A`, bit 12.
    #[bits(12..=12)]
    mrst_tx_a: bool,

    /// `LRST_RX_B`, bit 11.
    #[bits(11..=11)]
    lrst_rx_b: bool,

    /// `MRST_RX_B`, bit 10.
    #[bits(10..=10)]
    mrst_rx_b: bool,

    /// `LRST_RX_A`, bit 9.
    #[bits(9..=9)]
    lrst_rx_a: bool,

    /// `MRST_RX_A`, bit 8.
    #[bits(8..=8)]
    mrst_rx_a: bool,

    /// `SRST_RXFIFO`, bit 7.
    #[bits(7..=7)]
    srst_rx_fifo: bool,

    /// `SRST_TXFIFO`, bit 6.
    #[bits(6..=6)]
    srst_tx_fifo: bool,

    /// Channel B's receive path enabled. `RXEN_B`, bit 5.
    #[bits(5..=5)]
    rx_b_enabled: bool,

    /// Channel A's receive path enabled. `RXEN_A`, bit 4.
    #[bits(4..=4)]
    rx_a_enabled: bool,

    /// Channel B's transmit path enabled. `TXEN_B`, bit 3.
    #[bits(3..=3)]
    tx_b_enabled: bool,

    /// Channel A's transmit path enabled. `TXEN_A`, bit 2.
    #[bits(2..=2)]
    tx_a_enabled: bool,

    /// The bank later accesses to banked registers reach. `MAC`, bits 1:0.
    #[bits(0..=1)]
    mac: Mac,
}

impl Register for LmlControl {
    const ADDR: Reg = Reg::LmlControl;
}

impl LmlControl {
    /// Every reset bit and every channel enable set, `mac` selected: the value
    /// `lms7002m_create` leaves (`0xFFFF` with both channels).
    #[must_use]
    pub const fn running(mac: Mac) -> Self {
        Self::from_raw(0xfffc).with_mac(mac)
    }
}

/// MIMO or SISO operation. `LML_0x002E`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MimoSiso {
    /// Channel B's digital part powered down (SISO). `MIMO_SISO`, bit 15; libusdr's
    /// `lms7002m_create` clears it, commenting "Enable MIMO, this bit only hardwires
    /// MAC\[1\]".
    #[bits(15..=15)]
    siso: bool,
}

impl Register for MimoSiso {
    const ADDR: Reg = Reg::MimoSiso;
}

/// The chip's identity, read only. `LML_0x002F`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Version {
    /// Chip version. `VER`, bits 15:11.
    #[bits(11..=15)]
    chip: u5,

    /// Chip revision. `REV`, bits 10:6.
    #[bits(6..=10)]
    revision: u5,

    /// Metal-mask revision. `MASK`, bits 5:0.
    #[bits(0..=5)]
    mask: u6,
}

impl Register for Version {
    const ADDR: Reg = Reg::Version;
}

/// The only version and revision libusdr's `lms7002m_create` accepts (`-ENODEV`
/// otherwise): `VER` 7, `REV` 1, any `MASK`.
pub const SUPPORTED: Version = Version::new()
    .with_chip(u5::new(7))
    .with_revision(u5::new(1));

impl Version {
    /// Whether `lms7002m_create` accepts this chip: `VER` and `REV` match [`SUPPORTED`];
    /// the mask is not checked (a board driver may check it: libusdr's `LimeSDR` requires 1).
    #[must_use]
    pub const fn is_supported(self) -> bool {
        self.chip().value() == SUPPORTED.chip().value()
            && self.revision().value() == SUPPORTED.revision().value()
    }

    /// `VER:REV` (bits 15:6), the identity an error reports.
    #[must_use]
    pub const fn id(self) -> u16 {
        self.to_raw() >> 6
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_leaves_every_lml_bit_set() {
        // MAKE_LMS7002M_LML_0x0020(1, 1, ..., 1, LMS7_CH_AB).
        assert_eq!(LmlControl::running(Mac::Both).to_raw(), 0xffff);
        assert_eq!(LmlControl::running(Mac::A).to_raw(), 0xfffd);
    }

    #[test]
    fn only_version_and_revision_decide_support() {
        assert!(
            Version::from_raw(0x3840).is_supported(),
            "VER 7, REV 1, MASK 0"
        );
        assert!(
            Version::from_raw(0x387f).is_supported(),
            "MASK is not checked"
        );
        assert!(!Version::from_raw(0x3880).is_supported(), "REV 2");
        assert!(!Version::from_raw(0x4040).is_supported(), "VER 8");
        assert_eq!(Version::from_raw(0x3841).id(), 0xe1);
    }
}
