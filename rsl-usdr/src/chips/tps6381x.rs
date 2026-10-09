//! TPS63811 buck-boost converter, which supplies a 3.45 V rail (libusdr's "DC-DC boost").
//!
//! # What it does
//!
//! A buck-boost converter regulates its output whether its input is above or below it.
//! Assumed, not documented in libusdr: it is fed from the M.2 slot's 3.3 V supply, below
//! the 3.45 V the uSDR asks for. The TPS63811 starts with its output
//! off (`ENABLE` resets to 0) until software turns it on.
//!
//! The part has two output-voltage registers, and its VSEL pin picks which one is in
//! force. The driver writes 3.45 V to both, so the board's wiring of VSEL does not matter.
//!
//! libusdr calls the part `I2C_DEV_DCDCBOOST` and does not say which loads it feeds. It is
//! programmed before the clocks start, and the separate booster GPO line
//! ([`Gpo::Booster`](crate::fpga::Gpo::Booster)) switches on later, just before the
//! LMS6002D leaves reset.
//!
//! # How the driver uses it
//!
//! [`Tps6381x::init`] checks the part is TI silicon revision B0, then enables it in forced
//! PWM (fixed-frequency switching, for the reason given in the [`lp8758`](super::lp8758)
//! docs) at 3.45 V.
//!
//! # Sources
//!
//! Register meanings: TI SLVSEK4C, "TPS63810, TPS63811", §8.6 (also libusdr's
//! `hw/tps6381x/tps6381x.yaml`). Values written: libusdr `hw/tps6381x/tps6381x.c`.

use bnb::{BitEnum, bitfield, u2, u4};

use super::register::{I2cRegisters, IndexedRegister, Register};
use crate::error::Error;
use crate::lowlevel::{Bus, I2cAddr};

/// Register addresses.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
pub(crate) enum Reg {
    /// See [`Control`]. `CONTROL`, §8.6.1.2.
    Control = 0x01,

    /// See [`DeviceId`]. `DEVID`, §8.6.1.4.
    DeviceId = 0x03,

    /// Output voltage while the VSEL pin is low. `VOUT1`, §8.6.1.5; reset 0x3C (3.3 V).
    VoltageVselLow = 0x04,

    /// Output voltage while the VSEL pin is high. `VOUT2`, §8.6.1.6; reset 0x42 (3.45 V).
    VoltageVselHigh = 0x05,
}

/// How fast the output moves to a new voltage.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
enum RampRate {
    /// 1.0 V/ms.
    VoltsPerMs1,

    /// 2.5 V/ms.
    VoltsPerMs2_5,

    /// 5.0 V/ms.
    VoltsPerMs5,

    /// 10.0 V/ms.
    VoltsPerMs10,
}

/// Turns the converter on and sets its range and switching. `CONTROL`, §8.6.1.2.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Control {
    /// Use the high output range (2.025 V to 5.2 V) instead of the low (1.8 V to
    /// 4.975 V). `RANGE`, bit 6; reset 0.
    #[bits(6..=6)]
    high_range: bool,

    /// Run the converter; the TPS63811 starts off. `ENABLE`, bit 5.
    #[bits(5..=5)]
    enabled: bool,

    /// Always switch in PWM. `FPWM`, bit 3; reset 0.
    #[bits(3..=3)]
    forced_pwm: bool,

    /// Switch in PWM while the output ramps to a new voltage. `RPWM`, bit 2; reset 0.
    #[bits(2..=2)]
    ramp_pwm: bool,

    /// Output ramp rate. `SLEW`, bits 1:0; reset 1.0 V/ms.
    #[bits(0..=1)]
    ramp_rate: RampRate,
}
impl Register for Control {
    type Map = Reg;
    const ADDR: Reg = Reg::Control;
}

/// Manufacturer and silicon revision. `DEVID`, §8.6.1.4 (read-only).
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DeviceId {
    /// Manufacturer: 0 is Texas Instruments. `MANUFACTURER`, bits 7:4.
    #[bits(4..=7)]
    manufacturer: u4,

    /// Major silicon revision: 0 is A, 1 is B. `MAJOR`, bits 3:2.
    #[bits(2..=3)]
    major: u2,

    /// Minor silicon revision. `MINOR`, bits 1:0.
    #[bits(0..=1)]
    minor: u2,
}
impl Register for DeviceId {
    type Map = Reg;
    const ADDR: Reg = Reg::DeviceId;
}

/// Which output-voltage register, chosen by the VSEL pin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Vsel {
    /// VSEL low: `VOUT1`.
    Low,
    /// VSEL high: `VOUT2`.
    High,
}

/// An output voltage in the low range, as its `VOUTn` code (1.8 V plus 25 mV per step).
/// Built in const context, so an out-of-range board constant fails to compile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OutputVoltage(u8);

impl OutputVoltage {
    /// Lowest low-range output, in millivolts.
    const MIN_MV: u32 = 1800;
    /// Highest low-range output, in millivolts.
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

impl IndexedRegister for OutputVoltage {
    type Map = Reg;
    type Index = Vsel;

    fn addr(vsel: Vsel) -> Reg {
        match vsel {
            Vsel::Low => Reg::VoltageVselLow,
            Vsel::High => Reg::VoltageVselHigh,
        }
    }
}

impl From<OutputVoltage> for u8 {
    fn from(value: OutputVoltage) -> Self {
        value.0
    }
}

/// A `TPS6381x` on the I2C bus.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Tps6381x {
    /// The chip's registers.
    regs: I2cRegisters,
}

impl Tps6381x {
    /// The ID libusdr requires: Texas Instruments, silicon revision B0.
    const DEVICE_ID: u8 = 0x04;

    /// The converter at `dev`.
    pub(crate) const fn at(dev: I2cAddr) -> Self {
        Self {
            regs: I2cRegisters::new(dev, "TPS6381x ID read", "TPS6381x write"),
        }
    }

    /// The converter where the uSDR wires it: FPGA I2C bus 0, address 0x75, the part's fixed
    /// address (`I2C_DEV_DCDCBOOST` in libusdr).
    pub(crate) const fn usdr() -> Self {
        Self::at(I2cAddr::new(0, 0x75))
    }

    /// Checks the device ID, then runs the converter at `voltage` whichever level VSEL is
    /// at (`tps6381x_init`).
    pub(crate) fn init(
        self,
        bus: &mut dyn Bus,
        forced_pwm: bool,
        voltage: OutputVoltage,
    ) -> Result<(), Error> {
        let id: DeviceId = self.regs.read(bus)?;
        let ti_revision_b0 =
            id.manufacturer().value() == 0 && id.major().value() == 1 && id.minor().value() == 0;
        if !ti_revision_b0 {
            return Err(Error::ChipId {
                chip: "TPS6381x",
                expected: Self::DEVICE_ID.into(),
                found: id.to_raw().into(),
            });
        }
        let control = Control::new()
            .with_enabled(true)
            .with_forced_pwm(forced_pwm)
            .with_ramp_pwm(forced_pwm);
        self.regs.write(bus, control)?;
        self.regs.write_at(bus, Vsel::Low, voltage)?;
        self.regs.write_at(bus, Vsel::High, voltage)
    }
}
