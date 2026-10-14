//! TI `ADS42LBx9` dual-channel ADC: the SPI frames and registers libusdr's
//! `hw/ads42lbx9/ads42lbx9.c` uses at open and close.
//!
//! The datasheet is not vendored; addresses and bits are libusdr's (`enum ads42lbx9`,
//! `MAKE_CTRL`). Frame, as `ads42lbx9_wr`/`_rd` build the SPI word: bit 15 set for a read,
//! the 6-bit register address in bits 13:8, data in bits 7:0; a read keeps the low byte of
//! the word returned. Chip-level code: it moves to a shared `devices/sdr/chips/ads42lbx9`
//! crate when a second driver needs it (`PORTING.md`).

use bnb::{BitEnum, bitfield};
use hal::{FpgaAccess, Register, SpiAddr};

/// The chip's name in errors.
pub(crate) const NAME: &str = "ADS42LBx9";

/// Bit 15 of the frame: read.
const READ: u32 = 0x8000;

/// The registers the `LSDr` touches.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
pub(crate) enum Reg {
    /// See [`PowerControl`]. `REG_PWR_CONTROL`.
    PowerControl = 0x08,

    /// The register `ads42lbx9_reset_and_check` reads to find the chip; see
    /// [`is_present`]. `REG_HF0`.
    Hf0 = 0x0d,
}

/// Powers the chip and its channels down, and resets it. `REG_PWR_CONTROL`, as libusdr's
/// `MAKE_CTRL` builds it.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PowerControl {
    /// Channel A powered down; `MAKE_CTRL`'s `chae` clear. Bit 7.
    #[bits(7..=7)]
    channel_a_down: bool,

    /// Channel B powered down; `MAKE_CTRL`'s `chbe` clear. Bit 6.
    #[bits(6..=6)]
    channel_b_down: bool,

    /// The chip powered down; `MAKE_CTRL`'s `pd`. Bit 5.
    #[bits(5..=5)]
    powered_down: bool,

    /// Reset; `MAKE_CTRL`'s `reset`. Bit 0.
    #[bits(0..=0)]
    reset: bool,
}

impl PowerControl {
    /// What close writes (`ads42lbx9_ctrl(.., false, false, true, false)`): both channels
    /// and the chip down, no reset (0xE0).
    pub(crate) const OFF: Self = Self::new()
        .with_channel_a_down(true)
        .with_channel_b_down(true)
        .with_powered_down(true)
        .with_reset(false);
}

impl Register for PowerControl {
    type Map = Reg;
    const ADDR: Reg = Reg::PowerControl;
}

/// The SPI exchange reading `reg` on the ADC at `target` (`ads42lbx9_rd`).
pub(crate) fn read(target: SpiAddr, reg: Reg) -> FpgaAccess {
    let addr = u8::from(reg);
    FpgaAccess::Spi {
        target,
        word: READ | u32::from(addr & 0x3f) << 8,
        reg: addr.into(),
        write: false,
    }
}

/// The SPI exchange writing `value` to the ADC at `target` (`ads42lbx9_wr`).
pub(crate) fn write<R: Register<Map = Reg>>(target: SpiAddr, value: R) -> FpgaAccess {
    let (reg, data) = value.entry();
    let addr = u8::from(reg);
    FpgaAccess::Spi {
        target,
        word: u32::from(addr & 0x3f) << 8 | u32::from(data),
        reg: addr.into(),
        write: true,
    }
}

/// Whether a [`Reg::Hf0`] read shows an `ADS42LBx9`: `ads42lbx9_reset_and_check` accepts
/// `(HF0 & 0xFE) == 0x6C` from the word's low byte and reports anything else absent.
pub(crate) const fn is_present(word: u32) -> bool {
    word.to_le_bytes()[0] & 0xfe == 0x6c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_put_the_address_in_bits_13_to_8_and_the_read_flag_in_15() {
        let target = SpiAddr(1);
        assert!(matches!(
            read(target, Reg::Hf0),
            FpgaAccess::Spi {
                word: 0x8d00,
                reg: 0x0d,
                write: false,
                ..
            }
        ));
        assert!(matches!(
            write(target, PowerControl::OFF),
            FpgaAccess::Spi {
                word: 0x08e0,
                reg: 0x08,
                write: true,
                ..
            }
        ));
    }

    #[test]
    fn presence_ignores_hf0_bit_0_and_everything_above_the_byte() {
        assert!(is_present(0x6c));
        assert!(is_present(0xffff_ff6d));
        assert!(!is_present(0x6e));
        assert!(!is_present(u32::MAX), "a floating bus is not an ADC");
    }
}
