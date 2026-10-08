//! The `m2_lm6_1` board: its state, how its chips are wired, and its supply setpoints.
//! The sequences that drive it are `impl Board` blocks in sibling modules.

use std::fmt;

use crate::chips::lms6002d::Lms6002d;
use crate::chips::lp8758::{BuckVoltage, Lp8758};
use crate::chips::si5332::Si5332;
use crate::chips::tmp114::Tmp114;
use crate::chips::tps6381x::{OutputVoltage, Tps6381x};
use crate::lowlevel::Bus;

/// A powered uSDR board and the bus it is reached through.
pub(crate) struct Board {
    /// The hardware seam; chip drivers borrow it per call.
    pub(super) bus: Box<dyn Bus>,
    /// The RF transceiver.
    pub(super) lms: Lms6002d,
}

impl fmt::Debug for Board {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Board")
            .field("lms", &self.lms)
            .finish_non_exhaustive()
    }
}

impl Board {
    /// The LP8758 PMIC.
    pub(super) const PMIC: Lp8758 = Lp8758::usdr();
    /// The TMP114 temperature sensor.
    pub(super) const TEMP: Tmp114 = Tmp114::usdr();
    /// The Si5332 clock generator.
    pub(super) const CLOCK: Si5332 = Si5332::usdr();
    /// The TPS63811 boost converter.
    pub(super) const BOOST: Tps6381x = Tps6381x::usdr();

    /// GPIO bank voltage (PMIC buck 1), compatible with xSDR.
    pub(super) const VGPIO: BuckVoltage = BuckVoltage::from_millivolts(1800);
    /// LMS6002D I/O rail (PMIC buck 3) at normal sample rates.
    pub(super) const LMS_VIO_NORMAL: BuckVoltage = BuckVoltage::from_millivolts(1800);
    /// Boost converter output, 3.45 V.
    pub(super) const BOOST_VOLTAGE: OutputVoltage = OutputVoltage::from_millivolts(3450);
}
