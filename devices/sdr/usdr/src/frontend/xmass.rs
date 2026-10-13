//! The XMASS synchronization board (libusdr's `xmass`, `device/ext_xmass/ext_xmass.c`): an
//! LMK05318 clock tree, a calibration source (the LMK05318's output 4, or a noise
//! generator) with its switches, and power amplifiers, behind a GPIO expander.
//!
//! `rsl-sdr-frontend` identifies the board by its expander and drives the expander
//! (`rsl_sdr_frontend::xmass`). Once it has, libusdr powers the LMK05318 and GPS, brings
//! the LMK05318 up with a 25 MHz reference on output 6 and the calibration LO on output 4
//! (444 MHz), waits for its APLLs, pulses its `SYNC_N`, then reads the revision-1a sensors
//! and serial number for its log; that bring-up is here.
//!
//! As in libusdr, a failed LMK05318 bring-up is logged and the board carries on to the lock
//! waits; they decide.

use std::time::Duration;

use rsl_deps::tracing::{info, warn};

use rsl_sdr_frontend::xmass;

use super::{FRONT_END_BUS, FrontEnd};
use crate::chips::at24::At24Security;
use crate::chips::lmk05318::Lmk05318;
use crate::chips::lmk05318::plan::{Affinity, OutputFormat, OutputRequest, Xo, XoInput};
use crate::chips::tmp114::Tmp114;
use crate::error::Error;
use crate::lowlevel::{Bus, I2cAddr};

/// The board's name in errors (libusdr's `xmass`).
pub(super) const NAME: &str = xmass::NAME;

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

/// An XMASS board that came up, with its clock synchronizer's state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Xmass {
    /// The clock synchronizer.
    lmk: Lmk05318,
    /// The calibration LO last requested, in Hz (`calfreq`).
    calibration_hz: u32,
}

impl Xmass {
    /// Brings up a board `rsl-sdr-frontend` found (`board_xmass_init` after its expander
    /// check): powers the LMK05318 to check it, brings it up and waits for its APLLs,
    /// pulses `SYNC_N` and logs the revision-1a sensors and serial number.
    ///
    /// # Errors
    ///
    /// [`Error::ClockUnlocked`] if an APLL does not lock; any bus failure.
    pub(super) fn bring_up(bus: &mut dyn Bus) -> Result<Self, Error> {
        xmass::power_up(bus)?;

        let mut board = Self {
            lmk: Lmk05318::at(xmass::LMK05318),
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
        board.lmk.log_lock(bus);
        locked?;

        // Pulse SYNC_N to bring the outputs into phase.
        xmass::pulse_sync(bus)?;
        info!("XMASS LMK05318 outputs synced");
        log_revision_1a(bus)?;
        Ok(board)
    }

    /// The calibration LO last requested, in Hz (`/dm/sync/cal/freq` read).
    pub(crate) const fn calibration_hz(&self) -> u32 {
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
    pub(super) fn tune_calibration(&mut self, bus: &mut dyn Bus, hz: u32) -> Result<(), Error> {
        self.calibration_hz = hz;
        let written = self
            .lmk
            .plan(&self.outputs())
            .and_then(|()| self.lmk.write_staged(bus));
        bus.sleep(Duration::from_millis(10));
        let locked = written.and_then(|()| self.lmk.wait_apll2_lock(bus, RETUNE_LOCK_TIMEOUT));
        self.lmk.log_lock(bus);
        locked
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

impl From<Xmass> for FrontEnd {
    fn from(board: Xmass) -> Self {
        Self::Xmass(Box::new(board))
    }
}
