//! Native error adapter around the shared, platform-independent USB I²C machine.

use core::time::Duration;

use super::protocol::I2cCache;
use crate::lowlevel::{BusError, I2cAddr};
use crate::portable::usb::i2c as core_i2c;

pub(super) use core_i2c::Action;

/// Native transfer events retain their original bus error through the portable machine.
pub(super) type Event<'a> = core_i2c::Event<'a, BusError>;

/// Host adapter with the pre-existing bus-error interface; all transitions live in the core.
pub(super) struct I2cTransaction(core_i2c::I2cTransaction<BusError>);

impl I2cTransaction {
    /// Prepares the bounded LUT/control packet, preserving existing native errors.
    pub(super) fn new(
        id: u64,
        dev: I2cAddr,
        write: &[u8],
        read_len: usize,
        cache: &mut I2cCache,
        now: Duration,
    ) -> Result<Self, BusError> {
        core_i2c::I2cTransaction::new(id, dev, write, read_len, cache, now)
            .map(Self)
            .map_err(BusError::from)
    }

    /// Hands the command to the executor exactly once.
    pub(super) fn start(&mut self, now: Duration) -> Result<Action, BusError> {
        self.0.start(now).map_err(BusError::from)
    }

    /// Advances caller-supplied elapsed time without an I/O callback.
    #[cfg(test)]
    pub(super) fn advance(&mut self, now: Duration) -> Result<Option<Duration>, BusError> {
        self.0.advance(now).map_err(BusError::from)
    }

    /// Refreshes a pending action's timeout immediately before native submission.
    pub(super) fn dispatch(&mut self, action: Action, now: Duration) -> Result<Action, BusError> {
        self.0.dispatch(action, now).map_err(BusError::from)
    }

    /// Applies one correlated native transfer result without changing its error source.
    pub(super) fn complete(
        &mut self,
        event: Event<'_>,
        now: Duration,
        next_sequence: &mut Option<u16>,
    ) -> Result<Action, BusError> {
        self.0
            .complete(event, now, next_sequence)
            .map_err(BusError::from)
    }

    /// Exercises the unchanged queued/issued cancellation contract in execution tests.
    #[cfg(test)]
    pub(super) fn cancel(&mut self, now: Duration) -> Result<(), BusError> {
        self.0.cancel(now).map_err(BusError::from)
    }
}
