//! The radio as a caller sees it.

use std::fmt;

use crate::board::Board;
use crate::error::Error;
use crate::lowlevel::Bus;

/// A powered uSDR.
///
/// Dropping a `Device` powers the board down on a best-effort basis; call
/// [`Device::close`] to see power-down errors.
pub struct Device {
    /// The hardware seam.
    bus: Box<dyn Bus>,
    /// Board state; `None` once powered down.
    board: Option<Board>,
}

impl Device {
    /// Powers up a board reached through a custom [`Bus`].
    ///
    /// # Errors
    ///
    /// Any bus failure, an unsupported board revision, or a chip that does not identify
    /// itself as expected.
    pub fn with_bus(bus: impl Bus + 'static) -> Result<Self, Error> {
        let mut bus: Box<dyn Bus> = Box::new(bus);
        let board = Board::power_up(bus.as_mut())?;
        Ok(Self {
            bus,
            board: Some(board),
        })
    }

    /// Powers the board down.
    ///
    /// # Errors
    ///
    /// The first bus failure; every power-down step is still attempted.
    pub fn close(mut self) -> Result<(), Error> {
        self.power_down()
    }

    /// Powers the board down if it is still up.
    fn power_down(&mut self) -> Result<(), Error> {
        self.board
            .take()
            .map_or(Ok(()), |board| board.power_down(self.bus.as_mut()))
    }
}

impl fmt::Debug for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Device")
            .field("powered", &self.board.is_some())
            .finish_non_exhaustive()
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        // Errors are reported only by `close`.
        let _ = self.power_down();
    }
}
