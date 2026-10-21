//! Platform-independent `LimeSDR` code: the LMS64C codec, the board code's low-level
//! operations and how the control channel carries them, the gateware registers, the
//! board's hooks for the LMS7002M control code, and the hand-written sans-I/O open, sample
//! rate, stream preparation and stream sync, and the stream framing, the blocking [`Device`](crate::Device) runs. The LMS7002M's
//! own operations come from `lms7002m`. Core only: no std, no allocation; keep imports
//! within this tree and the shared `no_std` crates.

pub(crate) mod fpga;
pub(crate) mod framing;
pub(crate) mod frontend;
pub mod lms64c;
pub(crate) mod lsop;
pub(crate) mod modify;
pub(crate) mod open;
pub(crate) mod rate;
pub(crate) mod reference;
pub(crate) mod stream;
pub(crate) mod sync;
