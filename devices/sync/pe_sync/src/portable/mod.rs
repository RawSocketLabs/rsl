//! Platform-independent board and chip sequences: the gateware registers, the clock chips'
//! registers and solvers, and the hand-written sans-I/O sequences the blocking
//! [`Device`](crate::Device) runs with `hal`'s executor. The LMK05318's plan, registers
//! and bus traffic come from `lmk05318`, whose blocking driver runs outside this tree (its
//! traffic does not fit the `Sequence` seam). Core only: no std, no allocation; keep
//! imports within this tree and the shared `no_std` crates.

pub(crate) mod board;
pub(crate) mod chain;
pub(crate) mod float;
pub(crate) mod fpga;
pub(crate) mod lmk1d1208i;
pub(crate) mod lmx1204;
pub(crate) mod lmx1214;
pub(crate) mod lmx2820;
pub(crate) mod outcome;
pub(crate) mod poll;
pub(crate) mod power;
pub(crate) mod script;
pub(crate) mod status;
#[cfg(test)]
pub(crate) mod testing;
pub(crate) mod ti_spi;
pub(crate) mod ti_sync;
