//! Platform-independent bring-up: the gateware registers, the LMS7002M's reset programme and
//! the hand-written sans-I/O sequences the blocking [`Device`](crate::Device) runs with
//! `rsl-lowlevel`'s executor. Core only: no std, no allocation; keep imports within this
//! tree and the shared `no_std` crates.

pub(crate) mod fpga;
pub(crate) mod hwid;
pub(crate) mod lms7002m;
pub(crate) mod outcome;
pub(crate) mod power;
pub(crate) mod supplies;
