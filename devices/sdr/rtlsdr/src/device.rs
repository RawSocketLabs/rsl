//! [`Device`]: one open dongle.

use crate::control::ControlPort;
use crate::error::Error;
use crate::portable::rtl2832u;

/// One RTL-SDR dongle behind a [`ControlPort`].
pub struct Device {
    /// The dongle's transport.
    #[expect(
        dead_code,
        reason = "driven once the open sequence (RTL-GAP-07) is evidenced"
    )]
    port: Box<dyn ControlPort>,
}

impl Device {
    /// Brings the dongle up over `port`.
    ///
    /// # Errors
    ///
    /// [`Error::Gap`] until the open sequence is evidenced (`RTL-GAP-07`); nothing reaches
    /// the port before then.
    pub fn open(port: Box<dyn ControlPort>) -> Result<Self, Error> {
        rtl2832u::OPEN.get()?;
        Ok(Self { port })
    }
}
