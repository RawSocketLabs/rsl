//! The XMASS synchronization board (libusdr's `xmass`, `device/ext_xmass/ext_xmass.c`):
//! an LMK05318 clock tree, a calibration source (the LMK05318's output 4, or a noise
//! generator) with its switches, and power amplifiers, behind a GPIO expander.
//!
//! libusdr identifies the board by its control expander: it writes a direction mask and
//! reads it back. It then powers the LMK05318 and GPS, brings the LMK05318 up with a
//! 25 MHz reference on output 6 and the calibration LO on output 4 (444 MHz), waits for
//! its APLLs, pulses its `SYNC_N`, then reads the revision-1a sensors and serial number for
//! its log.
//!
//! As in libusdr, a failed LMK05318 bring-up is logged and the board carries on to the lock
//! waits; they decide.

use std::time::Duration;

use deps::tracing::{info, warn};
use hal::{Bus, I2cAddr};
use lmk05318::plan::{Affinity, OutputFormat, OutputRequest, Xo, XoInput};

use crate::FRONT_END_BUS;
use crate::chips::at24::At24Security;
use crate::chips::lmk05318::{Lmk05318, log_lock};
use crate::chips::tca9555::{Reg, Tca9555};
use crate::chips::tmp114::Tmp114;
use crate::error::Error;
use crate::gpio::{FrontEndGpio, GpioCommand, PinMode};

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
const LMK05318: I2cAddr = I2cAddr::new(FRONT_END_BUS, 0x65);

/// How long libusdr lets the LMK05318 power up before talking to it.
const POWER_UP: Duration = Duration::from_millis(250);

/// Revision 1a's first temperature sensor, a TMP114NA (`I2C_TMP114NA`).
const SENSOR_A: Tmp114 = Tmp114::at(I2cAddr::new(FRONT_END_BUS, 0x4f));

/// Revision 1a's second temperature sensor, a TMP114NB (`I2C_TMP114NB`).
const SENSOR_B: Tmp114 = Tmp114::at(I2cAddr::new(FRONT_END_BUS, 0x4e));

/// Revision 1a's EEPROM security registers (`I2C_DEV_AT24_SEC`).
const EEPROM: At24Security = At24Security::at(I2cAddr::new(FRONT_END_BUS, 0x58));

/// The reference XMASS distributes, from output 6, in Hz (`refclk`).
const REFERENCE_HZ: u32 = 25_000_000;

/// The calibration LO libusdr starts with, on output 4, in Hz (`calfreq`).
const INITIAL_CALIBRATION_HZ: u32 = 444_000_000;

/// The LMK05318's XO: 26 MHz, AC-coupled differential with external termination.
const XO: Xo = Xo {
    hz: 26_000_000,
    input: XoInput::AcDifferentialExternal,
    detector_bypass: false,
};

/// How long the bring-up waits for each APLL to lock (100 ms).
const BRING_UP_LOCK_TIMEOUT: Duration = Duration::from_millis(100);

/// How long a calibration retune waits for APLL2 to lock (10 ms).
const RETUNE_LOCK_TIMEOUT: Duration = Duration::from_millis(10);

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
}

/// An XMASS board that came up, with its clock synchronizer's state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Xmass {
    /// The clock synchronizer.
    lmk: Lmk05318,
    /// The calibration LO last requested, in Hz (`calfreq`).
    calibration_hz: u32,
}

impl Xmass {
    /// Looks for the board and brings it up (`board_xmass_init`): its control expander
    /// checked, the LMK05318 and GPS powered, the LMK05318 brought up and its APLLs waited
    /// for, `SYNC_N` pulsed, and the revision-1a sensors and serial number logged. `None`
    /// when the expander does not keep its direction mask.
    ///
    /// # Errors
    ///
    /// [`Error::ClockUnlocked`] if an APLL does not lock; any bus failure.
    pub(crate) fn init(bus: &mut dyn Bus) -> Result<Option<Self>, Error> {
        if !detect(bus)? {
            return Ok(None);
        }
        EXPANDER.write16(bus, Reg::Output0, NORMAL)?;
        bus.sleep(POWER_UP);

        let mut board = Self {
            lmk: Lmk05318::at(LMK05318),
            calibration_hz: INITIAL_CALIBRATION_HZ,
        };
        if let Err(error) = board.lmk.create(bus, XO, &board.outputs()) {
            warn!(%error, "unable to initialize XMASS's LMK05318; carrying on as libusdr does");
        }
        // The DPLL is off, so there is no reference validation to wait for.
        let locked = board
            .lmk
            .wait_apll1_lock(bus, BRING_UP_LOCK_TIMEOUT)
            .and_then(|()| board.lmk.wait_apll2_lock(bus, BRING_UP_LOCK_TIMEOUT));
        log_lock(&board.lmk, bus);
        locked?;

        // Pulse SYNC_N to bring the outputs into phase.
        EXPANDER.write16(bus, Reg::Output0, NORMAL & !SYNC_N)?;
        EXPANDER.write16(bus, Reg::Output0, NORMAL)?;
        info!("XMASS LMK05318 outputs synced");
        log_revision_1a(bus)?;
        Ok(Some(board))
    }

    /// The calibration LO last requested, in Hz (`/dm/sync/cal/freq` read).
    #[must_use]
    pub const fn calibration_hz(&self) -> u32 {
        self.calibration_hz
    }

    /// Retunes the calibration LO on output 4 to `hz` (`board_xmass_tune_cal_lo`), or turns
    /// it off below 3.1 MHz, then waits for APLL2 and logs the lock status.
    ///
    /// As in libusdr the request is kept even when it fails, and the pause and log follow
    /// a failed plan.
    ///
    /// # Errors
    ///
    /// [`Error::ClockPlan`] when the LMK05318 cannot reach `hz` (nothing is written);
    /// [`Error::ClockUnlocked`] if APLL2 does not lock; any bus failure.
    pub fn tune_calibration(&mut self, bus: &mut dyn Bus, hz: u32) -> Result<(), Error> {
        self.calibration_hz = hz;
        let written = self
            .lmk
            .plan(&self.outputs())
            .and_then(|()| self.lmk.write_staged(bus));
        bus.sleep(Duration::from_millis(10));
        let locked = written.and_then(|()| self.lmk.wait_apll2_lock(bus, RETUNE_LOCK_TIMEOUT));
        log_lock(&self.lmk, bus);
        Ok(locked?)
    }

    /// Switches the calibration path to `path` (`board_xmass_sync_source`).
    ///
    /// # Errors
    ///
    /// Any bus failure.
    pub fn set_calibration_path(
        &self,
        bus: &mut dyn Bus,
        path: XmassCalibrationPath,
    ) -> Result<(), Error> {
        EXPANDER.write16(bus, Reg::Output0, path.outputs())
    }

    /// The LMK05318's outputs for the current calibration LO
    /// (`_board_xmass_fill_lmk05318`): only output 4 (the LO, from APLL2, off below
    /// 3.1 MHz) and output 6 (the reference, from APLL1) run.
    fn outputs(&self) -> [OutputRequest; 8] {
        // The C compares the integer with the double 3.1e6.
        let lo = if f64::from(self.calibration_hz) < 3.1e6 {
            0
        } else {
            self.calibration_hz
        };
        let lo_format = if lo == 0 {
            OutputFormat::Off
        } else {
            OutputFormat::Lvds
        };
        let off = |port| OutputRequest::new(port, 0, OutputFormat::Off);
        [
            off(0),
            off(1),
            off(2),
            off(3),
            OutputRequest::new(4, lo, lo_format).with_affinity(Affinity::Apll2),
            off(5),
            OutputRequest::new(6, REFERENCE_HZ, OutputFormat::LvcmosPN)
                .with_affinity(Affinity::Apll1),
            off(7).with_affinity(Affinity::Apll1),
        ]
    }
}

/// Looks for the board (`board_xmass_init` up to its expander check): configures the
/// connector's pins, then writes the expander's direction mask and reads it back. libusdr
/// reads the directions once before writing them, and ignores that first value.
fn detect(bus: &mut dyn Bus) -> Result<bool, Error> {
    FrontEndGpio::run(bus, &SETUP)?;
    EXPANDER.read16(bus, Reg::Config0)?;
    EXPANDER.write16(bus, Reg::Config0, DIRECTIONS)?;
    Ok(EXPANDER.read16(bus, Reg::Config0)? == DIRECTIONS)
}

/// Logs revision 1a's temperatures and serial number, as libusdr does: each sensor's
/// temperature if it identifies itself, then the serial number if either did.
fn log_revision_1a(bus: &mut dyn Bus) -> Result<(), Error> {
    let mut found = false;
    for (name, sensor) in [("A", SENSOR_A), ("B", SENSOR_B)] {
        if sensor.identifies(bus)? {
            found = true;
            let celsius = sensor.celsius(bus)?;
            info!(sensor = name, celsius, "XMASS revision 1a temperature");
        }
    }
    if found {
        let serial = EEPROM.serial_number(bus)?;
        info!(?serial, "XMASS serial number");
    }
    Ok(())
}
