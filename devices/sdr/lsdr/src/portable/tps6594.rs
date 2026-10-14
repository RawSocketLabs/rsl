//! TI TPS6594 power-management IC: the registers and values libusdr's `hw/tps6594/tps6594.c`
//! writes on the `LSDr`.
//!
//! The TPS6594's datasheet is not vendored. Addresses and bit names are libusdr's
//! (`enum tps6594_regs`, `reg6594_buckx_ctrl`, `reg6594_ldox_ctrl`); output codes follow its
//! `tps6594_buck_vout`, unchecked against TI's tables. Chip-level code: it moves to a shared
//! `devices/sdr/chips/tps6594` crate when a second driver needs it (`PORTING.md`).

use bnb::{BitEnum, bitfield};
use hal::{IndexedRegister, Register};

/// The chip's name in errors.
pub(crate) const NAME: &str = "TPS6594";

/// The registers the `LSDr` touches.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
pub(crate) enum Reg {
    /// Device revision. `DEV_REV`; open reads and logs it.
    DeviceRevision = 0x01,

    /// NVM code, first byte. `NVM_CODE_1`; open reads and logs it.
    NvmCode1 = 0x02,

    /// NVM code, second byte. `NVM_CODE_2`; open reads and logs it.
    NvmCode2 = 0x03,

    /// See [`BuckVoltage`], buck 1. `BUCK1_VOUT_1`.
    Buck1Voltage = 0x0e,

    /// Buck 2's output voltage. `BUCK2_VOUT_1`.
    Buck2Voltage = 0x10,

    /// Buck 3's output voltage. `BUCK3_VOUT_1`.
    Buck3Voltage = 0x12,

    /// Buck 4's output voltage. `BUCK4_VOUT_1`.
    Buck4Voltage = 0x14,

    /// See [`Ldo4Control`]. `LDO4_CTRL`.
    Ldo4Control = 0x20,
}

/// The identification bytes `tps6594_check` reads, in its order. libusdr logs them and
/// checks nothing.
pub(crate) const IDENTIFICATION: [Reg; 3] = [Reg::DeviceRevision, Reg::NvmCode1, Reg::NvmCode2];

/// One buck converter (`TPS6594_BUCK1`..`4`; the chip's fifth is unused on the `LSDr`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Buck {
    /// Buck 1.
    B1,
    /// Buck 2.
    B2,
    /// Buck 3.
    B3,
    /// Buck 4.
    B4,
}

/// A buck's output voltage code. `BUCKn_VOUT_1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BuckVoltage(u8);

impl BuckVoltage {
    /// libusdr's code for `millivolts` (`tps6594_buck_vout`), rounded down to the step: 20 mV
    /// from 0.3 V (code 0x00), 5 mV from 0.6 V (0x0F), 10 mV from 1.1 V (0x73), 20 mV from
    /// 1.66 V (0xAB); below 0.3 V code 0x00, from 3.34 V code 0xFF.
    pub(crate) const fn from_millivolts(millivolts: u32) -> Self {
        let code = match millivolts {
            3340.. => 0xff,
            1660.. => (millivolts - 1660) / 20 + 0xab,
            1100.. => (millivolts - 1100) / 10 + 0x73,
            600.. => (millivolts - 600) / 5 + 0x0f,
            300.. => (millivolts - 300) / 20,
            _ => 0,
        };
        #[expect(
            clippy::cast_possible_truncation,
            reason = "every branch stays within 0xFF: 0xAB + 1679 / 20 = 0xFE"
        )]
        let code = code as u8;
        Self(code)
    }
}

impl IndexedRegister for BuckVoltage {
    type Map = Reg;
    type Index = Buck;

    fn addr(buck: Buck) -> Reg {
        match buck {
            Buck::B1 => Reg::Buck1Voltage,
            Buck::B2 => Reg::Buck2Voltage,
            Buck::B3 => Reg::Buck3Voltage,
            Buck::B4 => Reg::Buck4Voltage,
        }
    }
}

impl From<BuckVoltage> for u8 {
    fn from(value: BuckVoltage) -> Self {
        value.0
    }
}

/// Switches LDO4. `LDO4_CTRL`; bit names are libusdr's (`reg6594_ldox_ctrl`), which also
/// names bit 7 (`LDOX_RV_SEL`) without using it.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Ldo4Control {
    /// Pull the output down while off. `LDOX_PLDN`, bit 5.
    #[bits(5..=5)]
    pull_down: bool,

    /// Monitor the output voltage. `LDOX_VMON_EN`, bit 4.
    #[bits(4..=4)]
    monitored: bool,

    /// Bit 1, which libusdr names only for the bucks (`BUCKX_FPWM`, forced PWM) and sets
    /// on every rail it enables, LDOs included; its meaning for an LDO is not in the source.
    #[bits(1..=1)]
    bit1: bool,

    /// Run the LDO. `LDOX_EN`, bit 0.
    #[bits(0..=0)]
    enabled: bool,
}

impl Ldo4Control {
    /// What `tps6594_vout_ctrl(.., TPS6594_LDO4, on)` writes: pull-down always; on adds
    /// enable, bit 1 and voltage monitoring (0x33), off is 0x20.
    pub(crate) const fn switched(on: bool) -> Self {
        Self::new()
            .with_pull_down(true)
            .with_enabled(on)
            .with_bit1(on)
            .with_monitored(on)
    }
}

impl Register for Ldo4Control {
    type Map = Reg;
    const ADDR: Reg = Reg::Ldo4Control;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buck_codes_follow_libusdrs_steps_at_each_boundary() {
        let code = |millivolts| u8::from(BuckVoltage::from_millivolts(millivolts));
        assert_eq!(code(0), 0x00);
        assert_eq!(code(599), 0x0e, "20 mV steps up to 0.6 V");
        assert_eq!(code(600), 0x0f);
        assert_eq!(code(1099), 0x72, "5 mV steps up to 1.1 V");
        assert_eq!(code(1100), 0x73);
        assert_eq!(code(1659), 0xaa, "10 mV steps up to 1.66 V");
        assert_eq!(code(1660), 0xab);
        assert_eq!(code(3339), 0xfe);
        assert_eq!(code(3340), 0xff);
        assert_eq!(code(u32::MAX), 0xff);
    }

    #[test]
    fn ldo4_switching_reproduces_libusdrs_bytes() {
        assert_eq!(u8::from(Ldo4Control::switched(false)), 0x20);
        assert_eq!(u8::from(Ldo4Control::switched(true)), 0x33);
    }
}
