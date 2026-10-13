//! The control channel: what a transport (the FT601 USB bridge, or a simulator) must
//! provide.
//!
//! **Unstable.** The `LimeSDR`'s firmware answers LMS64C frames ([`lms64c`](crate::lms64c));
//! a transport moves one request and its reply at a time and keeps their order. Which
//! frames carry the board code's operations is the driver's business, not the
//! transport's. No hardware transport exists yet: libusdr's FT601 one is not in the pinned
//! source.

use std::time::Duration;

pub use rsl_lowlevel::BusError;

use crate::lms64c::FRAME_LEN;

/// One board's LMS64C control channel, plus time.
pub trait ControlPort: Send {
    /// Sends one request frame and returns the board's reply frame.
    ///
    /// # Errors
    ///
    /// Any transport failure.
    fn exchange(&mut self, request: &[u8; FRAME_LEN]) -> Result<[u8; FRAME_LEN], BusError>;

    /// Waits for `duration`. Board code never reads the clock itself, so a simulator can
    /// make this virtual.
    fn sleep(&mut self, duration: Duration);
}
