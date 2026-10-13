//! The `m2_lm6_1` board: how its chips are wired and the sequences that drive them.

mod baseband;
mod board;
mod calibration;
mod correction;
mod frontend;
mod gain;
mod generator;
mod path;
mod power;
mod rate;
mod stream;
mod tune;
mod tx_stream;

pub use baseband::NcoChannels;
pub(crate) use board::Board;
pub use calibration::{Calibration, CalibrationReport, Feedback, IqCalibration, TxLoLeakage};
pub use correction::{RxIqCorrection, RxTrim, TxDcOffset, TxIqCorrection};
pub use gain::{RxGain, TxGain};
pub use path::{RxPath, TxPath};
pub(crate) use rate::RateChains;
pub use stream::{RxPacket, RxThrottle};
