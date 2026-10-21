//! Platform-independent board sequences: the gateware registers and the hand-written
//! sans-I/O sequences the blocking [`Device`](crate::Device) runs with `hal`'s
//! executor. The LMS7002M's own operations come from `lms7002m`. Core only: no std, no
//! allocation; keep imports within this tree and the shared `no_std` crates.

pub(crate) mod close;
pub(crate) mod fpga;
pub(crate) mod frontend;
pub(crate) mod hwid;
pub(crate) mod mmcm;
pub(crate) mod outcome;
pub(crate) mod power;
pub(crate) mod rate;
pub(crate) mod step;
pub(crate) mod supplies;
pub(crate) mod txcal;
pub(crate) mod xtrx;
