//! `TPS6381x` buck-boost converter (source: `hw/tps6381x/tps6381x.c`).

use bnb::{bitfield, u2};

use crate::error::{BusContext, Error};
use crate::lowlevel::{Bus, I2cAddr};

/// Control register.
const CONTROL: u8 = 0x01;
/// Device ID register.
const DEVID: u8 = 0x03;
/// Output voltage register 1.
const VOUT1: u8 = 0x04;
/// Output voltage register 2.
const VOUT2: u8 = 0x05;
/// The ID a TPS63811 reports.
const TPS63811_ID: u8 = 0x04;
/// Lowest settable output, in millivolts; `VOUTn` counts 25 mV steps from here.
const VOUT_MIN_MV: u32 = 1800;
/// Highest settable output, in millivolts.
const VOUT_MAX_MV: u32 = 4975;

/// The control register (`def_tps6381x.h`).
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

/// A valid output voltage, as its `VOUTn` code. Built in const context, so an
/// out-of-range board constant fails to compile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Vout(u8);

impl Vout {
    /// The code for `millivolts`, rounded down to a 25 mV step.
    ///
    /// # Panics
    ///
    /// Outside 1800..=4975 mV (at compile time when used in a `const`).
    pub(crate) const fn from_millivolts(millivolts: u32) -> Self {
        assert!(
            millivolts >= VOUT_MIN_MV && millivolts <= VOUT_MAX_MV,
            "TPS6381x output out of range"
        );
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the range check bounds the code to 127"
        )]
        let code = ((millivolts - VOUT_MIN_MV) / 25) as u8;
        Self(code)
    }
}

/// A `TPS6381x` on the I2C bus.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tps6381x {
    /// Where the converter answers.
    dev: I2cAddr,
}

impl Tps6381x {
    /// The converter at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self { dev }
    }

    /// Checks the ID, then enables the converter at `vout` (`tps6381x_init`).
    pub(crate) fn init(self, bus: &mut dyn Bus, force_pwm: bool, vout: Vout) -> Result<(), Error> {
        let mut id = [0];
        bus.i2c(self.dev, &[DEVID], &mut id)
            .during("TPS6381x ID read")?;
        if id[0] != TPS63811_ID {
            return Err(Error::ChipId {
                chip: "TPS6381x",
                expected: TPS63811_ID.into(),
                found: id[0].into(),
            });
        }
        let control = Control::new()
            .with_enable(true)
            .with_fpwm(force_pwm)
            .with_rpwm(force_pwm);
        for (reg, value) in [
            (CONTROL, control.to_raw()),
            (VOUT1, vout.0),
            (VOUT2, vout.0),
        ] {
            bus.i2c(self.dev, &[reg, value], &mut [])
                .during("TPS6381x write")?;
        }
        Ok(())
    }
}
