//! The `m2_lm6_1` board: how its chips are wired and the sequences that drive them.

mod board;
mod calibration;
mod correction;
mod debug;
mod frontend;
mod generator;
mod power;
mod rate;
mod stream;
mod tune;
mod tx_stream;

pub(crate) use crate::portable::radio::RateChains;
pub use crate::portable::radio::gain::{RxGain, TxGain};
pub use crate::portable::radio::path::{RxPath, TxPath};
pub use crate::portable::radio::state::NcoChannels;
pub(crate) use board::Board;
#[cfg(all(feature = "mio", target_os = "linux"))]
pub(crate) use board::Corrections;
#[cfg(all(feature = "mio", target_os = "linux"))]
pub(crate) use calibration::Calibrate;
pub use calibration::{Calibration, CalibrationReport, Feedback, IqCalibration, TxLoLeakage};
pub use correction::{RxIqCorrection, RxTrim, TxDcOffset, TxIqCorrection};
pub use debug::DebugChip;
pub(crate) use stream::Direction;
pub use stream::{RxPacket, RxThrottle};
#[cfg(all(feature = "mio", target_os = "linux"))]
pub(crate) use tx_stream::TxLaneUse;
