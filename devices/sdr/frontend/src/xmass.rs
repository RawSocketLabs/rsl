//! The XMASS synchronization board's identification and control expander (libusdr's
//! `xmass`, `device/ext_xmass/ext_xmass.c`).
//!
//! libusdr identifies the board by its control expander: it writes a direction mask and
//! reads it back. The expander then powers the board's LMK05318 and GPS, pulses the
//! LMK05318's `SYNC_N` and switches the calibration path; this module owns those writes.
//! The LMK05318 bring-up, lock waits and revision-1a sensors are the driver's.

use std::time::Duration;

use hal::{Bus, I2cAddr};

use crate::FRONT_END_BUS;
use crate::chips::tca9555::{Reg, Tca9555};
use crate::error::Error;
use crate::gpio::{FrontEndGpio, GpioCommand, PinMode};

/// The board's name in errors (libusdr's `xmass`).
pub const NAME: &str = "xmass";

/// The control expander, an `XRA1201` driven as a TCA9555 (`I2C_ADDR_XRA1201`).
const EXPANDER: Tca9555 = Tca9555::at(I2cAddr::new(FRONT_END_BUS, 0x14));

/// The connector setup: its I2C lines as inputs, then 1PPS in and the SYSREF generator
/// out.
const SETUP: [GpioCommand; 4] = [
    GpioCommand::Configure(0, PinMode::Input),
    GpioCommand::Configure(1, PinMode::Input),
    GpioCommand::Configure(2, PinMode::Alternate0),
    GpioCommand::Configure(3, PinMode::Alternate0),
];

/// The direction mask libusdr writes and expects back: port 1's pins 13 to 15 inputs
/// (the RTS lines), every other pin an output (`out_msk`).
const DIRECTIONS: u16 = 0xe000;

/// The expander outputs libusdr drives in normal operation (`ext_xmass_ctrl.yaml`):
/// distribution to other boards and locally (`BDISTRIB`, `BLOCAL`, bits 0-1), GPS power
/// (`GPS_PWREN`, 4), LMK05318 `SYNC_N` released (`LMK_SYNCN`, 5), 1PPS from SDR A
/// (`SYSREF_1PPS_SEL`, 6), the LMK05318 on (`EN_LMX`, 7) and the power amplifiers on
/// (`RF_EN`, 8).
const NORMAL: u16 = 0b11 | 1 << 4 | 1 << 5 | 1 << 6 | 1 << 7 | 1 << 8;

/// `LMK_SYNCN`: the LMK05318's `SYNC_N` pin, active low.
const SYNC_N: u16 = 1 << 5;

/// The calibration path's switches (`ext_xmass_ctrl.yaml`, P0/P1).
mod switch {
    /// Calibration into LNA3 rather than the general RX port (`RF_CAL_DST_SEL`, bit 2).
    pub(super) const TO_LNA3: u16 = 1 << 2;
    /// The noise generator rather than the LMK05318's LO (`RF_CAL_SRC_SEL`, bit 3).
    pub(super) const FROM_NOISE: u16 = 1 << 3;
    /// The loopback path to the SDR's RX (`RF_LB_SW`, bit 10).
    pub(super) const LOOPBACK: u16 = 1 << 10;
    /// The noise generator's supply (`RF_NOISE_EN`, bit 11).
    pub(super) const NOISE_ON: u16 = 1 << 11;
}

/// The board's clock synchronizer, an LMK05318 (`I2C_ADDR_LMK`).
pub const LMK05318: I2cAddr = I2cAddr::new(FRONT_END_BUS, 0x65);

/// How long libusdr lets the LMK05318 power up before talking to it.
const POWER_UP: Duration = Duration::from_millis(250);

/// Where XMASS sends its calibration signal (libusdr's `/dm/sdr/0/sync/cal/path`,
/// `board_xmass_sync_source`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum XmassCalibrationPath {
    /// No calibration signal: normal operation (0).
    #[default]
    Off,

    /// The LMK05318's calibration LO into the SDR's RX port (1).
    Lo,

    /// The noise generator into the SDR's RX port (2).
    Noise,

    /// The calibration LO into LNA3 (3).
    LoToLna3,

    /// The noise generator into LNA3 (4).
    NoiseToLna3,
}

impl XmassCalibrationPath {
    /// The expander outputs for this path.
    const fn outputs(self) -> u16 {
        NORMAL
            | match self {
                Self::Off => 0,
                Self::Lo => switch::LOOPBACK,
                Self::Noise => switch::LOOPBACK | switch::NOISE_ON | switch::FROM_NOISE,
                Self::LoToLna3 => switch::LOOPBACK | switch::TO_LNA3,
                Self::NoiseToLna3 => {
                    switch::LOOPBACK | switch::TO_LNA3 | switch::NOISE_ON | switch::FROM_NOISE
                }
            }
    }

    /// Switches XMASS's calibration path to this one (`board_xmass_sync_source`).
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub fn select(self, bus: &mut dyn Bus) -> Result<(), Error> {
        EXPANDER.write16(bus, Reg::Output0, self.outputs())
    }
}

/// Looks for the board (`board_xmass_init` up to its expander check): configures the
/// connector's pins, then writes the expander's direction mask and reads it back. libusdr
/// reads the directions once before writing them, and ignores that first value.
///
/// # Errors
///
/// Any bus failure.
pub(crate) fn detect(bus: &mut dyn Bus) -> Result<bool, Error> {
    FrontEndGpio::run(bus, &SETUP)?;
    EXPANDER.read16(bus, Reg::Config0)?;
    EXPANDER.write16(bus, Reg::Config0, DIRECTIONS)?;
    Ok(EXPANDER.read16(bus, Reg::Config0)? == DIRECTIONS)
}

/// Powers a found board's LMK05318, GPS and amplifiers, with `SYNC_N` released, and waits
/// for them to power up, as `board_xmass_init` does before bringing the LMK05318 up.
///
/// # Errors
///
/// Any bus failure.
pub fn power_up(bus: &mut dyn Bus) -> Result<(), Error> {
    EXPANDER.write16(bus, Reg::Output0, NORMAL)?;
    bus.sleep(POWER_UP);
    Ok(())
}

/// Pulses the LMK05318's `SYNC_N` low to bring its outputs into phase.
///
/// # Errors
///
/// Any bus failure.
pub fn pulse_sync(bus: &mut dyn Bus) -> Result<(), Error> {
    EXPANDER.write16(bus, Reg::Output0, NORMAL & !SYNC_N)?;
    EXPANDER.write16(bus, Reg::Output0, NORMAL)
}
