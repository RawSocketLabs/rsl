//! LP8758 four-channel step-down converter (PMIC), which supplies the board's core, I/O
//! and RF chip rails.
//!
//! # What it does
//!
//! The LP8758 has four buck converters, each turning the board's input supply into one
//! lower, regulated voltage. TI programs each variant's defaults into one-time-programmable
//! memory (OTP). The "-E0" variant on the uSDR resets with every channel enabled under
//! EN-pin control (see [`BuckControl`]'s reset values), which is how the board's rails are
//! up before any software runs. Over I2C the driver can then move each channel's voltage,
//! turn it on or off, and choose how it switches.
//!
//! On the uSDR the channels feed ([`Buck`] has the detail):
//!
//! | Channel | Rail | Set by the driver |
//! |---------|------|-------------------|
//! | 0 | 1.0 V (reset value 0x4D) | no |
//! | 1 | FPGA GPIO bank | 1.8 V |
//! | 2 | 1.2 V, per a libusdr comment | no |
//! | 3 | LMS6002D digital I/O | 1.8 V, 1.925 V at high sample rates |
//!
//! libusdr's comment calls channel 1 "2v5", but its code sets 1.8 V; this follows the
//! code.
//!
//! # How the driver uses it
//!
//! Power-up checks the revision ([`Lp8758::check_revision`]), turns spread spectrum off
//! and the 105 °C warning on ([`Config`]), sets channels 1 and 3 ([`BuckVoltage`]), then
//! runs all four in forced PWM ([`BuckControl`]). Forced PWM keeps each converter
//! switching at one fixed frequency even at light load. In the other mode, PFM, the
//! switching frequency moves with the load to save power, so its ripple can land anywhere,
//! including inside the band being received. libusdr does not state its reason; this is
//! the usual one for a radio.
//!
//! # Sources
//!
//! Register meanings: TI SNVSAC6B, "LP8758-E0", §7.6 (the board reports OTP revision 0xE0).
//! Values written: libusdr `hw/lp8758/lp8758.c`.

use bnb::{BitEnum, bitfield};

use super::register::{I2cRegisters, IndexedRegister, Register};
use crate::error::Error;
use crate::lowlevel::{Bus, I2cAddr};

/// Register addresses.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
pub(crate) enum Reg {
    /// Device revision. libusdr reads it as `DEV_REV`; SNVSAC6B's map does not list 0x00.
    DeviceRevision = 0x00,
    /// OTP image revision. `OTP_REV`, §7.6.1.1.
    OtpRevision = 0x01,
    /// See [`BuckControl`], channel 0. `BUCK0_CTRL1`.
    Buck0Control = 0x02,
    /// Channel 1 control. `BUCK1_CTRL1`.
    Buck1Control = 0x04,
    /// Channel 2 control. `BUCK2_CTRL1`.
    Buck2Control = 0x06,
    /// Channel 3 control. `BUCK3_CTRL1`.
    Buck3Control = 0x08,
    /// See [`BuckVoltage`], channel 0. `BUCK0_VOUT`.
    Buck0Voltage = 0x0a,
    /// Channel 1 output voltage. `BUCK1_VOUT`.
    Buck1Voltage = 0x0c,
    /// Channel 2 output voltage. `BUCK2_VOUT`.
    Buck2Voltage = 0x0e,
    /// Channel 3 output voltage. `BUCK3_VOUT`.
    Buck3Voltage = 0x10,
    /// See [`Config`]. `CONFIG`, §7.6.1.23.
    Config = 0x17,
}

/// One converter channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Buck {
    /// Channel 0: the 1.0 V rail (its OTP default is 1.0 V; libusdr leaves it there).
    B0,
    /// Channel 1: the GPIO bank rail, set to 1.8 V.
    B1,
    /// Channel 2: libusdr's comment calls it the 1.2 V rail; it never sets its voltage.
    B2,
    /// Channel 3: the LMS6002D I/O rail, 1.8 V at normal sample rates (libusdr raises it
    /// to 1.925 V above 62 MS/s).
    B3,
}

impl Buck {
    /// Every channel, in register order.
    pub(crate) const ALL: [Self; 4] = [Self::B0, Self::B1, Self::B2, Self::B3];
}

/// Turns a channel on or off and sets how it switches. `BUCKn_CTRL1`, §7.6.1.2.
///
/// libusdr writes three values: on (`PMIC_CH_ENABLE`, 0x88), on with forced PWM (0x8A),
/// and its "disable" (`PMIC_CH_DISABLE`, 0xC8), which in fact leaves the channel on under
/// EN-pin control.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BuckControl {
    /// Run the channel. `EN_BUCKn`, bit 7; reset 1 (OTP).
    #[bits(7..=7)]
    enabled: bool,
    /// Also require the selected EN pin to be high. `EN_PIN_CTRLn`, bit 6; reset 1 (OTP).
    #[bits(6..=6)]
    pin_controlled: bool,
    /// The EN pin is EN2 rather than EN1, when pin-controlled. `EN_PIN_SELECTn`, bit 5;
    /// reset 0 (OTP).
    #[bits(5..=5)]
    uses_en2: bool,
    /// The EN pin switches between roof and floor voltages instead of on and off.
    /// `EN_ROOF_FLOORn`, bit 4; reset 0.
    #[bits(4..=4)]
    pin_selects_roof_floor: bool,
    /// Discharge the output through a resistor while off. `EN_RDISn`, bit 3; reset 1.
    #[bits(3..=3)]
    discharge_when_off: bool,
    /// Always switch in PWM instead of moving between PFM and PWM. `BUCKn_FPWM`, bit 1;
    /// reset 0 (OTP).
    #[bits(1..=1)]
    forced_pwm: bool,
}

impl IndexedRegister for BuckControl {
    type Map = Reg;
    type Index = Buck;

    fn addr(buck: Buck) -> Reg {
        match buck {
            Buck::B0 => Reg::Buck0Control,
            Buck::B1 => Reg::Buck1Control,
            Buck::B2 => Reg::Buck2Control,
            Buck::B3 => Reg::Buck3Control,
        }
    }
}

/// A channel's output voltage. `BUCKn_VOUT` (`BUCKn_VSET`), §7.6.1.10; channel 0 resets
/// to 0x4D (1.0 V).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BuckVoltage(u8);

impl BuckVoltage {
    /// The code for `millivolts`, rounded down to the step: 10 mV from 0.5 V (code 0x00)
    /// to 0.73 V, 5 mV from 0.735 V (0x18) to 1.4 V (0x9D), 20 mV from 1.42 V (0x9E) to
    /// 3.36 V (0xFF).
    ///
    /// Divergence: libusdr's `s_get_vout` sits one code high between 0.735 V and 1.4 V and
    /// returns 0xFF above 3.33 V; this follows SNVSAC6B. The board uses only 1.8 V, where both
    /// agree.
    ///
    /// # Panics
    ///
    /// Outside 500..=3360 mV (at compile time when used in a `const`).
    pub(crate) const fn from_millivolts(millivolts: u32) -> Self {
        assert!(
            millivolts >= 500 && millivolts <= 3360,
            "LP8758 output out of range"
        );
        let code = match millivolts {
            1420.. => 0x9e + (millivolts - 1420) / 20,
            735.. => 0x18 + (millivolts - 735) / 5,
            _ => (millivolts - 500) / 10,
        };
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the range check bounds the code to 0xFF"
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
            Buck::B0 => Reg::Buck0Voltage,
            Buck::B1 => Reg::Buck1Voltage,
            Buck::B2 => Reg::Buck2Voltage,
            Buck::B3 => Reg::Buck3Voltage,
        }
    }
}

impl From<BuckVoltage> for u8 {
    fn from(value: BuckVoltage) -> Self {
        value.0
    }
}

/// Die-temperature warning level, EN-pin pull-downs, and spread spectrum. `CONFIG`,
/// §7.6.1.23.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Config {
    /// Raise the die-temperature warning at 105 °C instead of 125 °C. `TDIE_WARN_LEVEL`,
    /// bit 3; reset 0.
    #[bits(3..=3)]
    warn_at_105c: bool,
    /// Pull the EN2 pin down. `EN2_PD`, bit 2; reset 1.
    #[bits(2..=2)]
    en2_pull_down: bool,
    /// Pull the EN1 pin down. `EN1_PD`, bit 1; reset 1.
    #[bits(1..=1)]
    en1_pull_down: bool,
    /// Spread the switching frequency to reduce EMI peaks. `EN_SPREAD_SPEC`, bit 0; reset 0.
    #[bits(0..=0)]
    spread_spectrum: bool,
}
impl Register for Config {
    type Map = Reg;
    const ADDR: Reg = Reg::Config;
}

/// An LP8758 on the I2C bus.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Lp8758 {
    /// The chip's registers.
    regs: I2cRegisters,
}

impl Lp8758 {
    /// The revision libusdr requires: OTP revision 0xE0, device revision 0x01.
    const REVISION: u16 = 0xe001;

    /// The PMIC at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self {
            regs: I2cRegisters::new(dev, "LP8758 read", "LP8758 write"),
        }
    }

    /// The PMIC where the uSDR wires it: FPGA I2C bus 0, the LP8758-E0's address 0x60
    /// (SNVSAC6B; `I2C_DEV_PMIC_FPGA` in libusdr).
    pub(crate) const fn usdr() -> Self {
        Self::at(I2cAddr::new(0, 0x60))
    }

    /// Checks the revision, `OTP_REV << 8 | DEV_REV` as libusdr composes it, is the one
    /// libusdr accepts.
    pub(crate) fn check_revision(self, bus: &mut dyn Bus) -> Result<(), Error> {
        let device = self.regs.read_raw(bus, Reg::DeviceRevision)?;
        let otp = self.regs.read_raw(bus, Reg::OtpRevision)?;
        let found = u16::from_be_bytes([otp, device]);
        if found != Self::REVISION {
            return Err(Error::ChipId {
                chip: "LP8758",
                expected: Self::REVISION.into(),
                found: found.into(),
            });
        }
        Ok(())
    }

    /// Writes the configuration register.
    pub(crate) fn configure(self, bus: &mut dyn Bus, config: Config) -> Result<(), Error> {
        self.regs.write(bus, config)
    }

    /// Sets a channel's output voltage.
    pub(crate) fn set_voltage(
        self,
        bus: &mut dyn Bus,
        buck: Buck,
        voltage: BuckVoltage,
    ) -> Result<(), Error> {
        self.regs.write_at(bus, buck, voltage)
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
    fn voltage_codes_follow_the_datasheet_table() {
        let code = |millivolts| u8::from(BuckVoltage::from_millivolts(millivolts));
        assert_eq!(code(500), 0x00);
        assert_eq!(code(730), 0x17);
        assert_eq!(code(735), 0x18);
        assert_eq!(code(1400), 0x9d);
        assert_eq!(code(1420), 0x9e);
        assert_eq!(code(1800), 0xb1, "the board's rail, where libusdr agrees");
        assert_eq!(code(3360), 0xff);
    }

    #[test]
    fn control_values_reproduce_libusdr() {
        let on = BuckControl::new()
            .with_enabled(true)
            .with_discharge_when_off(true);
        assert_eq!(u8::from(on), 0x88, "PMIC_CH_ENABLE");
        assert_eq!(u8::from(on.with_forced_pwm(true)), 0x8a);
        assert_eq!(
            u8::from(on.with_pin_controlled(true)),
            0xc8,
            "PMIC_CH_DISABLE"
        );
    }
}
