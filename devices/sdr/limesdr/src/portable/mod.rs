//! Platform-independent `LimeSDR` code: the LMS64C codec, the board code's low-level
//! operations and how the control channel carries them, the gateware registers, and the
//! hand-written sans-I/O open the blocking [`Device`](crate::Device) runs. The LMS7002M's
//! own operations come from `rsl-lms7002m`. Core only: no std, no allocation; keep imports
//! within this tree and the shared `no_std` crates.

pub(crate) mod fpga;
pub mod lms64c;
pub(crate) mod lsop;
pub(crate) mod open;
pub(crate) mod reference;
