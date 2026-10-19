//! The front end on the board's M.2 connector: the probe at the end of power-up
//! (`usdr_device_m2_lm6_1_initialize`'s `device_fe_probe`) and the controls that reach it.

use sdr_frontend::Pciefe;

use super::Board;
use crate::error::Error;
use crate::frontend::{
    self, FrontEnd, FrontEndKind, FrontEndSelection, PciefeSetting, SYNC_CALIBRATION_BOARDS, XMASS,
    XmassCalibrationPath,
};

impl Board {
    /// Looks for a front end as `selection` says and brings it up.
    pub(crate) fn probe_front_end(&mut self, selection: &FrontEndSelection) -> Result<(), Error> {
        self.front_end = frontend::probe(self.bus.as_mut(), selection)?;
        Ok(())
    }

    /// The front end brought up, if any.
    pub(crate) fn front_end(&self) -> Option<FrontEndKind> {
        self.front_end.as_ref().map(FrontEnd::kind)
    }

    /// Sets the front end's oscillator DAC (`dev_m2_lm6_1_sdr_vctcxo_set`): the `PCIe` front
    /// end's, or the breakout's if it answered.
    pub(crate) fn set_oscillator_dac(&mut self, code: u16) -> Result<(), Error> {
        let bus = self.bus.as_mut();
        match &self.front_end {
            Some(FrontEnd::Pciefe(_)) => Ok(Pciefe::set_dac(bus, code)?),
            Some(FrontEnd::Exm2pe(board)) => Ok(board.set_dac(bus, code)?),
            // The other boards have none (libusdr's `-EINVAL`).
            _ => Err(Error::NoOscillatorDac),
        }
    }

    /// Applies one `PCIe` front-end control (`/debug/hw/pciefe_cmd/0/reg`).
    pub(crate) fn configure_pciefe(&mut self, setting: PciefeSetting) -> Result<(), Error> {
        let Some(FrontEnd::Pciefe(board)) = &mut self.front_end else {
            return Err(Error::FrontEndMissing("pciefe"));
        };
        Ok(board.apply(self.bus.as_mut(), setting)?)
    }

    /// Retunes a synchronization board's calibration or measurement LO
    /// (`/dm/sync/cal/freq`): XMASS's output 4, `SimpleSync`'s outputs 0 to 3.
    pub(crate) fn set_sync_calibration_hz(&mut self, hz: u32) -> Result<(), Error> {
        let bus = self.bus.as_mut();
        match &mut self.front_end {
            Some(FrontEnd::Xmass(board)) => Ok(board.tune_calibration(bus, hz)?),
            Some(FrontEnd::SimpleSync(board)) => Ok(board.tune_lo(bus, hz)?),
            _ => Err(Error::FrontEndMissing(SYNC_CALIBRATION_BOARDS)),
        }
    }

    /// The XMASS calibration LO last requested (`/dm/sync/cal/freq` read).
    pub(crate) fn sync_calibration_hz(&self) -> Result<u32, Error> {
        let Some(FrontEnd::Xmass(board)) = &self.front_end else {
            return Err(Error::FrontEndMissing(XMASS));
        };
        Ok(board.calibration_hz())
    }

    /// Switches XMASS's calibration path (`/dm/sdr/0/sync/cal/path`).
    pub(crate) fn set_sync_calibration_path(
        &mut self,
        path: XmassCalibrationPath,
    ) -> Result<(), Error> {
        let Some(FrontEnd::Xmass(board)) = &self.front_end else {
            return Err(Error::FrontEndMissing(XMASS));
        };
        Ok(board.set_calibration_path(self.bus.as_mut(), path)?)
    }
}
