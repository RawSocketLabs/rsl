//! The `SimpleSync` clock board (libusdr's `simplesync`,
//! `device/ext_simplesync/ext_simplesync.c`): an LMK05318 fanning a disciplined reference
//! out to four SDRs.
//!
//! Outputs 4 to 7 carry a 25 MHz reference each (`CLK B`, `C`, `D`, `A`), from APLL1;
//! outputs 0 to 3 (`RF D`, `A`, `B`, `C`) carry an optional measurement LO from APLL2, off
//! until one is requested.
//!
//! `rsl-sdr-frontend` finds the board (pins, power, the LMK05318's ID); the LMK05318
//! bring-up and the LO controls are here.

use std::time::Duration;

use rsl_deps::tracing::info;

use rsl_sdr_frontend::simplesync::LMK05318;

use super::FrontEnd;
use crate::chips::lmk05318::Lmk05318;
use crate::chips::lmk05318::plan::{Affinity, OutputFormat, OutputRequest, Xo, XoInput};
use crate::error::Error;
use crate::lowlevel::Bus;

/// The LMK05318's XO: 26 MHz LVCMOS.
const XO: Xo = Xo {
    hz: 26_000_000,
    input: XoInput::Cmos,
    detector_bypass: false,
};

/// The reference on outputs 4 to 7, in Hz.
const REFERENCE_HZ: u32 = 25_000_000;

/// The lowest measurement LO; below it outputs 0 to 3 are turned off (`LO_FREQ_CUTOFF`:
/// APLL2's lowest VCO over its largest post-divider and output divider, 3.07 MHz, rounded
/// up), in Hz.
const LO_CUTOFF_HZ: u32 = 3_500_000;

/// How long each lock wait lasts (10 ms).
const LOCK_TIMEOUT: Duration = Duration::from_millis(10);

/// The measurement LO's outputs.
const LO_OUTPUTS: [u8; 4] = [0, 1, 2, 3];

/// A `SimpleSync` board that came up, with its clock synchronizer's state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SimpleSync {
    /// The clock synchronizer.
    lmk: Lmk05318,
}

impl SimpleSync {
    /// Brings up a board `rsl-sdr-frontend` found (`board_ext_simplesync_init` after
    /// `lmk05318_create`'s ID check): the LMK05318 with the references on outputs 4 to 7,
    /// the LO outputs off, both APLLs locked and the outputs synchronized.
    ///
    /// # Errors
    ///
    /// [`Error::ClockUnlocked`] if an APLL does not lock; any bus failure.
    pub(super) fn bring_up(bus: &mut dyn Bus) -> Result<Self, Error> {
        let references = [4, 5, 6, 7].map(|port| {
            OutputRequest::new(port, REFERENCE_HZ, OutputFormat::LvcmosPN)
                .with_affinity(Affinity::Apll1)
        });
        let mut lmk = Lmk05318::at(LMK05318);
        lmk.configure(bus, XO, &references)?;
        let board = Self { lmk };
        let ready = board
            .power_down_lo(bus)
            .and_then(|()| board.lmk.wait_apll1_lock(bus, LOCK_TIMEOUT))
            .and_then(|()| board.lmk.wait_apll2_lock(bus, LOCK_TIMEOUT))
            .and_then(|()| board.lmk.sync(bus));
        board.lmk.log_lock(bus);
        ready?;
        info!("SimpleSync LMK05318 initialized");
        Ok(board)
    }

    /// Puts a measurement LO of `hz` on outputs 0 to 3 from APLL2, or turns them off below
    /// 3.5 MHz (`simplesync_tune_lo`), waiting for APLL2 and synchronizing the outputs.
    ///
    /// # Errors
    ///
    /// [`Error::ClockPlan`] when the LMK05318 cannot reach `hz` (nothing is written);
    /// [`Error::ClockUnlocked`] if APLL2 does not lock; any bus failure.
    pub(super) fn tune_lo(&mut self, bus: &mut dyn Bus, hz: u32) -> Result<(), Error> {
        if hz < LO_CUTOFF_HZ {
            return self.power_down_lo(bus);
        }
        let lo = LO_OUTPUTS.map(|port| {
            OutputRequest::new(port, hz, OutputFormat::Lvds).with_affinity(Affinity::Apll2)
        });
        self.lmk.plan(&lo)?;
        self.lmk.write_staged(bus)?;
        let synced = self
            .lmk
            .wait_apll2_lock(bus, LOCK_TIMEOUT)
            .and_then(|()| self.lmk.sync(bus));
        self.lmk.log_lock(bus);
        synced
    }

    /// Turns outputs 0 to 3 off (`simplesync_pd_low_chs`).
    fn power_down_lo(&self, bus: &mut dyn Bus) -> Result<(), Error> {
        LO_OUTPUTS
            .into_iter()
            .try_for_each(|port| self.lmk.disable_output(bus, port))
    }
}

impl From<SimpleSync> for FrontEnd {
    fn from(board: SimpleSync) -> Self {
        Self::SimpleSync(Box::new(board))
    }
}
