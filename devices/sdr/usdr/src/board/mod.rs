//! The `m2_lm6_1` board: how its chips are wired and the sequences that drive them.

mod board;
mod power;
mod rate;
mod stream;
mod tune;

pub(crate) use board::Board;
pub use stream::RxPacket;
