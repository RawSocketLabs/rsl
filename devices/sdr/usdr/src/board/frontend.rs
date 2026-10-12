//! The front end on the board's M.2 connector: the probe at the end of power-up
//! (`usdr_device_m2_lm6_1_initialize`'s `device_fe_probe`).

use super::Board;
use crate::error::Error;
use crate::frontend::{self, FrontEnd, FrontEndKind, FrontEndSelection};

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
}
