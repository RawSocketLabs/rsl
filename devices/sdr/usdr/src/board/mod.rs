//! The `m2_lm6_1` board: how its chips are wired and the sequences that drive them.

mod board;
mod correction;
mod frontend;
mod gain;
mod path;
mod power;
mod rate;
mod stream;
mod tune;

pub(crate) use board::Board;
pub use correction::{RxIqCorrection, RxTrim};
pub use gain::RxGain;
pub use path::RxPath;
pub use stream::{RxPacket, RxThrottle};
