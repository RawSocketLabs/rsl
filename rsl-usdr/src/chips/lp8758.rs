//! LP8758 four-channel buck PMIC (source: `hw/lp8758/lp8758.c`).

use crate::error::{BusContext, Error};
use crate::lowlevel::{Bus, I2cAddr};

/// Device revision register.
const DEV_REV: u8 = 0x00;
/// OTP revision register.
const OTP_REV: u8 = 0x01;
/// `BUCKn_CTRL1` for n = 0..=3.
const BUCK_CTRL1: [u8; 4] = [0x02, 0x04, 0x06, 0x08];
/// `BUCKn_VOUT` for n = 0..=3.
const BUCK_VOUT: [u8; 4] = [0x0a, 0x0c, 0x0e, 0x10];
/// Soft-start and thermal configuration register.
const CONFIG: u8 = 0x17;
/// [`CONFIG`] with soft start off and thermal shutdown at 125 °C.
const CONFIG_SOFT_START_OFF: u8 = 0x08;
/// [`CONFIG`] with soft start on and thermal shutdown at 125 °C.
const CONFIG_SOFT_START_ON: u8 = 0x09;
/// `BUCKn_CTRL1` value enabling the channel (`PMIC_CH_ENABLE`).
const CHANNEL_ENABLE: u8 = 0x88;
/// Forced-PWM bit in `BUCKn_CTRL1`.
const FORCE_PWM: u8 = 0x02;

/// One buck channel of the PMIC.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Buck {
    /// Channel 0.
    B0 = 0,
    /// Channel 1.
    B1 = 1,
    /// Channel 2.
    B2 = 2,
    /// Channel 3.
    B3 = 3,
}

impl Buck {
    /// Every channel, in register order.
    pub(crate) const ALL: [Self; 4] = [Self::B0, Self::B1, Self::B2, Self::B3];
}

/// An LP8758 on the I2C bus.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Lp8758 {
    /// Where the PMIC answers.
    dev: I2cAddr,
}

impl Lp8758 {
    /// The PMIC at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self { dev }
    }

    /// The revision as libusdr composes it: `OTP_REV << 8 | DEV_REV`.
    pub(crate) fn revision(self, bus: &mut dyn Bus) -> Result<u16, Error> {
        let dev_rev = self.read(bus, DEV_REV)?;
        let otp_rev = self.read(bus, OTP_REV)?;
        Ok(u16::from_be_bytes([otp_rev, dev_rev]))
    }

    /// Enables or disables soft start.
    pub(crate) fn set_soft_start(self, bus: &mut dyn Bus, enable: bool) -> Result<(), Error> {
        let config = if enable {
            CONFIG_SOFT_START_ON
        } else {
            CONFIG_SOFT_START_OFF
        };
        self.write(bus, CONFIG, config)
    }

    /// Sets a channel's output voltage.
    pub(crate) fn set_vout(
        self,
        bus: &mut dyn Bus,
        buck: Buck,
        millivolts: u32,
    ) -> Result<(), Error> {
        self.write(bus, BUCK_VOUT[buck as usize], vout_code(millivolts))
    }

    /// Enables a channel, optionally in forced-PWM mode.
    pub(crate) fn enable(
        self,
        bus: &mut dyn Bus,
        buck: Buck,
        force_pwm: bool,
    ) -> Result<(), Error> {
        let pwm = if force_pwm { FORCE_PWM } else { 0 };
        self.write(bus, BUCK_CTRL1[buck as usize], CHANNEL_ENABLE | pwm)
    }

    /// Writes one register.
    fn write(self, bus: &mut dyn Bus, reg: u8, value: u8) -> Result<(), Error> {
        bus.i2c(self.dev, &[reg, value], &mut [])
            .during("LP8758 write")
    }

    /// Reads one register.
    fn read(self, bus: &mut dyn Bus, reg: u8) -> Result<u8, Error> {
        let mut value = [0];
        bus.i2c(self.dev, &[reg], &mut value)
            .during("LP8758 read")?;
        Ok(value[0])
    }
}

/// The `BUCKn_VOUT` code for a voltage: 10 mV steps to 0.73 V, 5 mV steps to 1.4 V,
/// then 20 mV steps; above 3.33 V saturates (`s_get_vout`).
fn vout_code(millivolts: u32) -> u8 {
    let code = match millivolts {
        3331.. => 0xff,
        1401..=3330 => 0x9d + (millivolts - 1400) / 20,
        731..=1400 => 0x18 + (millivolts - 730) / 5,
        501..=730 => (millivolts - 500) / 10,
        _ => 0,
    };
    u8::try_from(code).expect("invariant: every branch stays within a byte")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vout_codes_match_the_c_formula_at_branch_edges() {
        assert_eq!(vout_code(1800), 0xb1);
        assert_eq!(vout_code(3330), 0xfd);
        assert_eq!(vout_code(3331), 0xff);
        assert_eq!(vout_code(1400), 0x18 + 134);
        assert_eq!(vout_code(730), 23);
        assert_eq!(vout_code(500), 0);
    }
}
