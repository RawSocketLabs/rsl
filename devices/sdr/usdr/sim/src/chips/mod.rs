//! Register-level models of the chips on the uSDR board.
//!
//! The front-end boards' chips, the plain register file and the TMP114 are
//! `sdr-frontend-sim`'s, shared with the other simulated boards; the board uses the last
//! two for its PMIC, boost converter and temperature sensor.

mod lms6002d;
mod si5332;

pub(crate) use lms6002d::Lms6002d;
pub use lms6002d::{DcCalibration, PllLock};
pub(crate) use sdr_frontend_sim::chips::{I2cChip, Reg8File, Tmp114};
pub(crate) use si5332::Si5332;
