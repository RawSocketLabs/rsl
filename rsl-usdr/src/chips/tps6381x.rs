//! `TPS6381x` buck-boost converter (source: `hw/tps6381x/tps6381x.c`, layouts from the
//! generated `def_tps6381x.h`).

use bnb::{BitEnum, bitfield, u2};

use super::register::{I2cRegisters, IndexedRegister, bitfield_register, enum_register};
use crate::error::Error;
use crate::lowlevel::{Bus, I2cAddr};

/// Register addresses.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
pub(crate) enum Reg {
    /// Control; see [`Control`].
    Control = 0x01,
    /// Device ID; see [`DeviceId`].
    DevId = 0x03,
    /// Output voltage 1.
    Vout1 = 0x04,
    /// Output voltage 2.
    Vout2 = 0x05,
}

/// The control register.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Control {
    /// Output voltage range select.
    #[bits(6..=6)]
    range: bool,
    /// Converter enable.
    #[bits(5..=5)]
    enable: bool,
    /// Forced PWM.
    #[bits(3..=3)]
    fpwm: bool,
    /// Forced PWM during ramp.
    #[bits(2..=2)]
    rpwm: bool,
    /// Output ramp slew rate.
    #[bits(0..=1)]
    slew: u2,
}

bitfield_register!(Control => Reg::Control);

/// The device ID register.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8)]
#[repr(u8)]
enum DeviceId {
    /// TPS63811, the part on the uSDR.
    Tps63811 = 0x04,
    /// Any other part.
    #[catch_all]
    Other(u8),
}

enum_register!(DeviceId => Reg::DevId);

/// Which output voltage register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VoutSelect {
    /// `VOUT1`.
    Vout1,
    /// `VOUT2`.
    Vout2,
}

/// An output voltage, as its `VOUTn` code. Built in const context, so an out-of-range
/// board constant fails to compile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Vout(u8);

impl Vout {
    /// Lowest settable output, in millivolts; codes count 25 mV steps from here.
    const MIN_MV: u32 = 1800;
    /// Highest settable output, in millivolts.
    const MAX_MV: u32 = 4975;

    /// The code for `millivolts`, rounded down to a 25 mV step.
    ///
    /// # Panics
    ///
    /// Outside 1800..=4975 mV (at compile time when used in a `const`).
    pub(crate) const fn from_millivolts(millivolts: u32) -> Self {
        assert!(
            millivolts >= Self::MIN_MV && millivolts <= Self::MAX_MV,
            "TPS6381x output out of range"
        );
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the range check bounds the code to 127"
        )]
        let code = ((millivolts - Self::MIN_MV) / 25) as u8;
        Self(code)
    }
}

impl IndexedRegister for Vout {
    type Map = Reg;
    type Index = VoutSelect;

    fn addr(select: VoutSelect) -> Reg {
        match select {
            VoutSelect::Vout1 => Reg::Vout1,
            VoutSelect::Vout2 => Reg::Vout2,
        }
    }

    fn to_byte(self) -> u8 {
        self.0
    }
}

/// A `TPS6381x` on the I2C bus.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tps6381x {
    /// The chip's registers.
    regs: I2cRegisters,
}

impl Tps6381x {
    /// The converter at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self {
            regs: I2cRegisters::new(dev, "TPS6381x ID read", "TPS6381x write"),
        }
    }

    /// Checks the ID, then enables the converter at `vout` (`tps6381x_init`).
    pub(crate) fn init(self, bus: &mut dyn Bus, force_pwm: bool, vout: Vout) -> Result<(), Error> {
        match self.regs.read::<DeviceId>(bus)? {
            DeviceId::Tps63811 => {}
            DeviceId::Other(found) => {
                return Err(Error::ChipId {
                    chip: "TPS6381x",
                    expected: u8::from(DeviceId::Tps63811).into(),
                    found: found.into(),
                });
            }
        }
        let control = Control::new()
            .with_enable(true)
            .with_fpwm(force_pwm)
            .with_rpwm(force_pwm);
        self.regs.write(bus, control)?;
        self.regs.write_at(bus, VoutSelect::Vout1, vout)?;
        self.regs.write_at(bus, VoutSelect::Vout2, vout)
    }
}
