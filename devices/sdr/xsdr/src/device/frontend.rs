//! The controls that reach the board's oscillator trim and the front end the open's probe
//! brought up: `m2_lm7_1.c`'s `/dm/sdr/0/dac_vctcxo` and the parameters `device_fe_probe`
//! registers for the board that came up (`device/device_fe.c`). The boards' code is
//! `sdr-frontend`'s, as on the uSDR.

use sdr_frontend::{FrontEnd, Pciefe, PciefeSetting, XmassCalibrationPath};

use super::Device;
use crate::error::Error;

/// The boards with a calibration LO, for [`Error::FrontEndMissing`].
const SYNC_CALIBRATION_BOARDS: &str = "xmass or simplesync";
/// XMASS's name, likewise.
const XMASS: &str = "xmass";
/// The `PCIe` front end's, likewise.
const PCIEFE: &str = "pciefe";

impl Device {
    /// Sets the reference oscillator's trim DAC to `code` (libusdr's
    /// `/dm/sdr/0/dac_vctcxo`): on an XSDR the `PCIe` front end's DAC, or the breakout's;
    /// on an XTRX its own trim DAC (a DAC80501's output code, or an MCP4725's top 12 bits).
    ///
    /// # Errors
    ///
    /// [`Error::NoOscillatorDac`] on an XSDR without either board (libusdr's `-ENOTSUP`),
    /// before any bus access; [`Error::FrontEnd`] for a breakout whose DAC did not answer
    /// at open (libusdr's `-EACCES`) or a front-end bus failure; [`Error::Bus`].
    pub fn set_oscillator_dac(&mut self, code: u16) -> Result<(), Error> {
        let bus = &mut *self.bus;
        match (&self.front_end, self.trim_dac) {
            (Some(FrontEnd::Pciefe(_)), _) => Ok(Pciefe::set_dac(bus, code)?),
            (Some(FrontEnd::Exm2pe(board)), _) => Ok(board.set_dac(bus, code)?),
            (_, Some(trim_dac)) => trim_dac.set(bus, code),
            _ => Err(Error::NoOscillatorDac),
        }
    }

    /// Applies one `PCIe` front-end control (libusdr's `/debug/hw/pciefe_cmd/0/reg`
    /// write).
    ///
    /// # Errors
    ///
    /// [`Error::FrontEndMissing`] without a `PCIe` front end, before any bus access;
    /// [`Error::FrontEnd`] for its failures.
    pub fn configure_pciefe(&mut self, setting: PciefeSetting) -> Result<(), Error> {
        let Some(FrontEnd::Pciefe(board)) = &mut self.front_end else {
            return Err(Error::FrontEndMissing(PCIEFE));
        };
        Ok(board.apply(&mut *self.bus, setting)?)
    }

    /// Retunes the synchronization board's calibration LO to `hz` (libusdr's
    /// `/dm/sync/cal/freq`, on XMASS also `/dm/sdr/0/sync/cal/freq`), waiting for its clock
    /// synthesizer to lock: XMASS's calibration source (off below 3.1 MHz), or
    /// `SimpleSync`'s measurement LO on its outputs 0 to 3 (off below 3.5 MHz). On XMASS
    /// the request is kept even if it fails, as libusdr keeps it.
    ///
    /// # Errors
    ///
    /// [`Error::FrontEndMissing`] without XMASS or `SimpleSync`, before any bus access;
    /// [`Error::FrontEnd`] for a frequency the clock synthesizer cannot reach (before
    /// writing it), one it does not lock on, or a bus failure.
    pub fn set_sync_calibration_frequency(&mut self, hz: u32) -> Result<(), Error> {
        let bus = &mut *self.bus;
        match &mut self.front_end {
            Some(FrontEnd::Xmass(board)) => Ok(board.tune_calibration(bus, hz)?),
            Some(FrontEnd::SimpleSync(board)) => Ok(board.tune_lo(bus, hz)?),
            _ => Err(Error::FrontEndMissing(SYNC_CALIBRATION_BOARDS)),
        }
    }

    /// The XMASS calibration LO last requested, in Hz (libusdr's `/dm/sync/cal/freq`
    /// read): 444 MHz after the open.
    ///
    /// # Errors
    ///
    /// [`Error::FrontEndMissing`] without XMASS (`SimpleSync` has no such reading in
    /// libusdr).
    pub fn sync_calibration_frequency(&self) -> Result<u32, Error> {
        let Some(FrontEnd::Xmass(board)) = &self.front_end else {
            return Err(Error::FrontEndMissing(XMASS));
        };
        Ok(board.calibration_hz())
    }

    /// Routes XMASS's calibration signal (libusdr's `/dm/sdr/0/sync/cal/path`).
    ///
    /// # Errors
    ///
    /// [`Error::FrontEndMissing`] without XMASS, before any bus access; [`Error::FrontEnd`]
    /// for a bus failure.
    pub fn set_sync_calibration_path(&mut self, path: XmassCalibrationPath) -> Result<(), Error> {
        let Some(FrontEnd::Xmass(board)) = &self.front_end else {
            return Err(Error::FrontEndMissing(XMASS));
        };
        Ok(board.set_calibration_path(&mut *self.bus, path)?)
    }
}
