//! The public device: open, power-up and close.

use std::fmt;

use crate::control::ControlPort;
use crate::error::{Error, ExchangeError};
use crate::lms64c::Frame;
use crate::portable::lsop::{LsOp, Refused};
use crate::portable::open::{Action, Open};

/// What libusdr reports at `/dm/revision` for this family: an emulated constant, the same
/// for every board (`dev_limesdr_sdr_revision_get`, which documents none of its fields).
const REVISION: u64 = (2 << 8) << 32 | (1 << 27 | 1 << 23 | 13 << 17 | 1 << 12 | 1 << 6 | 1);

/// An open LimeSDR-mini.
pub struct Device {
    /// The board's control channel.
    port: Box<dyn ControlPort>,

    /// The nominal reference frequency measured at open, in hertz.
    reference_hz: u32,
}

impl Device {
    /// Opens the board on `port`, as libusdr's `limesdr_init` does: measures the reference
    /// clock, pulses the LMS7002M's reset, checks its version, loads its reset programme
    /// and stops streaming.
    ///
    /// # Errors
    ///
    /// [`Error::DetectorStalled`] or [`Error::UnknownReference`] for a reference clock that
    /// cannot be measured or recognised, before the LMS7002M is touched;
    /// [`Error::ChipId`] for an LMS7002M version word other than 0x3841, before anything is
    /// written to it; [`Error::Bus`] or [`Error::Reset`] when an exchange fails.
    #[expect(
        clippy::missing_panics_doc,
        reason = "invariant: an open finishes only after recognising its reference"
    )]
    pub fn open(port: impl ControlPort + 'static) -> Result<Self, Error> {
        let mut port: Box<dyn ControlPort> = Box::new(port);
        let mut open = Open::new();
        while let Some(action) = open.next() {
            let result = match action {
                Action::Op(op) => exchange(&mut *port, op),
                Action::Sleep(duration) => {
                    port.sleep(duration);
                    Ok(0)
                }
            };
            open.complete(result)?;
        }
        let reference_hz = open
            .reference_hz()
            .expect("invariant: a finished open has recognised its reference");
        Ok(Self { port, reference_hz })
    }

    /// Powers the board up as libusdr's `/dm/power/en` = 1 does on this family: not at all
    /// (`dev_limesdr_pwren_set` returns at once). The open has already brought the LMS7002M
    /// up.
    ///
    /// # Errors
    ///
    /// None yet; fallible for the power control a later port may add.
    pub fn power_up(&mut self) -> Result<(), Error> {
        Ok(())
    }

    /// Closes the board as libusdr's `usdr_dmd_close` does on this family: without a bus
    /// operation (`limesdr_dtor` is a "TODO"), leaving the LMS7002M as the open left it and
    /// releasing the control channel.
    ///
    /// # Errors
    ///
    /// None yet; fallible for the stream shutdown a later port adds.
    pub fn close(self) -> Result<(), Error> {
        drop(self.port);
        Ok(())
    }

    /// The nominal reference-clock frequency measured at open, in hertz: 10, 30.72, 38.4,
    /// 40 or 52 MHz (the LimeSDR-mini's own is 40 MHz).
    #[must_use]
    pub const fn reference_hz(&self) -> u32 {
        self.reference_hz
    }

    /// The board revision libusdr reports (`/dm/revision`): a constant it emulates rather
    /// than reads, the same for every LimeSDR-mini.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        REVISION
    }
}

impl fmt::Debug for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Device")
            .field("reference_hz", &self.reference_hz)
            .finish_non_exhaustive()
    }
}

/// Performs `op` as one LMS64C exchange on `port`: what it returned.
fn exchange(port: &mut dyn ControlPort, op: LsOp) -> Result<u32, ExchangeError> {
    let reply = port.exchange(&op.request().to_bytes())?;
    op.reply(&Frame::from_bytes(reply))
        .map_err(|Refused(status)| ExchangeError::Refused(status))
}
