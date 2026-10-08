//! LP8758 four-channel buck PMIC (source: `hw/lp8758/lp8758.c`).
//!
//! libusdr has no register map for this chip, only the values it writes, so its control
//! registers are typed by those values rather than by bit fields.

use bnb::BitEnum;

use super::register::{I2cRegisters, IndexedRegister, enum_register};
use crate::error::Error;
use crate::lowlevel::{Bus, I2cAddr};

/// Register addresses.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
pub(crate) enum Reg {
    /// Device revision.
    DevRev = 0x00,
    /// OTP revision.
    OtpRev = 0x01,
    /// Channel 0 control.
    Buck0Ctrl1 = 0x02,
    /// Channel 1 control.
    Buck1Ctrl1 = 0x04,
    /// Channel 2 control.
    Buck2Ctrl1 = 0x06,
    /// Channel 3 control.
    Buck3Ctrl1 = 0x08,
    /// Channel 0 output voltage.
    Buck0Vout = 0x0a,
    /// Channel 1 output voltage.
    Buck1Vout = 0x0c,
    /// Channel 2 output voltage.
    Buck2Vout = 0x0e,
    /// Channel 3 output voltage.
    Buck3Vout = 0x10,
    /// Soft-start and thermal configuration.
    Config = 0x17,
}

/// One buck channel of the PMIC.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Buck {
    /// Channel 0.
    B0,
    /// Channel 1.
    B1,
    /// Channel 2.
    B2,
    /// Channel 3.
    B3,
}

impl Buck {
    /// Every channel, in register order.
    pub(crate) const ALL: [Self; 4] = [Self::B0, Self::B1, Self::B2, Self::B3];
}

/// `BUCKn_CTRL1`, by the values libusdr writes (`PMIC_CH_ENABLE`, `PMIC_CH_DISABLE`, and
/// bit 1 for forced PWM). Write-only here, so a closed set.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
pub(crate) enum BuckControl {
    /// Channel on, automatic PFM/PWM.
    Enabled = 0x88,
    /// Channel on, forced PWM.
    EnabledForcedPwm = 0x8a,
    /// Channel off.
    Disabled = 0xc8,
}

impl IndexedRegister for BuckControl {
    type Map = Reg;
    type Index = Buck;

    fn addr(buck: Buck) -> Reg {
        match buck {
            Buck::B0 => Reg::Buck0Ctrl1,
            Buck::B1 => Reg::Buck1Ctrl1,
            Buck::B2 => Reg::Buck2Ctrl1,
            Buck::B3 => Reg::Buck3Ctrl1,
        }
    }

    fn to_byte(self) -> u8 {
        self.into()
    }
}

/// `BUCKn_VOUT`: an output voltage code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BuckVout(u8);

impl BuckVout {
    /// The code for `millivolts`: 10 mV steps to 0.73 V, 5 mV steps to 1.4 V, then 20 mV
    /// steps; above 3.33 V saturates (`s_get_vout`).
    pub(crate) fn from_millivolts(millivolts: u32) -> Self {
        let code = match millivolts {
            3331.. => 0xff,
            1401..=3330 => 0x9d + (millivolts - 1400) / 20,
            731..=1400 => 0x18 + (millivolts - 730) / 5,
            501..=730 => (millivolts - 500) / 10,
            _ => 0,
        };
        Self(u8::try_from(code).expect("invariant: every branch stays within a byte"))
    }
}

impl IndexedRegister for BuckVout {
    type Map = Reg;
    type Index = Buck;

    fn addr(buck: Buck) -> Reg {
        match buck {
            Buck::B0 => Reg::Buck0Vout,
            Buck::B1 => Reg::Buck1Vout,
            Buck::B2 => Reg::Buck2Vout,
            Buck::B3 => Reg::Buck3Vout,
        }
    }

    fn to_byte(self) -> u8 {
        self.0
    }
}

/// The configuration register at 0x17, by the values libusdr writes (`lp8758_ss`).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8)]
#[repr(u8)]
pub(crate) enum Config {
    /// Soft start off.
    SoftStartOff = 0x08,
    /// Soft start on.
    SoftStartOn = 0x09,
    /// Any other value.
    #[catch_all]
    Other(u8),
}

enum_register!(Config => Reg::Config);

/// An LP8758 on the I2C bus.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Lp8758 {
    /// The chip's registers.
    regs: I2cRegisters,
}

impl Lp8758 {
    /// The PMIC at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self {
            regs: I2cRegisters::new(dev, "LP8758 read", "LP8758 write"),
        }
    }

    /// The revision as libusdr composes it: `OTP_REV << 8 | DEV_REV`.
    pub(crate) fn revision(self, bus: &mut dyn Bus) -> Result<u16, Error> {
        let dev_rev = self.regs.read_raw(bus, Reg::DevRev)?;
        let otp_rev = self.regs.read_raw(bus, Reg::OtpRev)?;
        Ok(u16::from_be_bytes([otp_rev, dev_rev]))
    }

    /// Writes the configuration register.
    pub(crate) fn configure(self, bus: &mut dyn Bus, config: Config) -> Result<(), Error> {
        self.regs.write(bus, config)
    }

    /// Sets a channel's output voltage.
    pub(crate) fn set_vout(
        self,
        bus: &mut dyn Bus,
        buck: Buck,
        vout: BuckVout,
    ) -> Result<(), Error> {
        self.regs.write_at(bus, buck, vout)
    }

    /// Writes a channel's control register.
    pub(crate) fn control(
        self,
        bus: &mut dyn Bus,
        buck: Buck,
        control: BuckControl,
    ) -> Result<(), Error> {
        self.regs.write_at(bus, buck, control)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vout_codes_match_the_c_formula_at_branch_edges() {
        let code = |millivolts| BuckVout::from_millivolts(millivolts).to_byte();
        assert_eq!(code(1800), 0xb1);
        assert_eq!(code(3330), 0xfd);
        assert_eq!(code(3331), 0xff);
        assert_eq!(code(1400), 0x18 + 134);
        assert_eq!(code(730), 23);
        assert_eq!(code(500), 0);
    }

    #[test]
    fn control_values_match_libusdr() {
        assert_eq!(BuckControl::Enabled.to_byte(), 0x88);
        assert_eq!(BuckControl::EnabledForcedPwm.to_byte(), 0x88 | 2);
        assert_eq!(BuckControl::Disabled.to_byte(), 0xc8);
    }
}
