//! The front end on the board's M.2 connector: the probe at the end of power-up
//! (`usdr_device_m2_lm6_1_initialize`'s `device_fe_probe`) and the controls that reach it.

use super::Board;
use crate::error::Error;
use crate::frontend::{self, FrontEnd, FrontEndKind, FrontEndSelection, PciefeSetting};

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

    /// Sets the front end's oscillator DAC (`dev_m2_lm6_1_sdr_vctcxo_set`).
    pub(crate) fn set_oscillator_dac(&mut self, code: u16) -> Result<(), Error> {
        let front_end = self.front_end.ok_or(Error::NoOscillatorDac)?;
        front_end.set_oscillator_dac(self.bus.as_mut(), code)
    }

    /// Applies one `PCIe` front-end control (`/debug/hw/pciefe_cmd/0/reg`).
    pub(crate) fn configure_pciefe(&mut self, setting: PciefeSetting) -> Result<(), Error> {
        let Some(FrontEnd::Pciefe(board)) = &mut self.front_end else {
            return Err(Error::FrontEndMissing("pciefe"));
        };
        board.apply(self.bus.as_mut(), setting)
    }
}
