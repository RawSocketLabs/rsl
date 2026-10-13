//! The `m2_lm6_1` board: how its chips are wired and the sequences that drive them.

mod board;
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

pub(crate) use board::Board;
pub use correction::{RxIqCorrection, RxTrim, TxDcOffset, TxIqCorrection};
pub use gain::{RxGain, TxGain};
pub use path::{RxPath, TxPath};
pub use stream::{RxPacket, RxThrottle};
