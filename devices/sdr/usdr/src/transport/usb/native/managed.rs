//! The managed synchronous adapter: a [`NativeUsb`] on a Mio reactor of its own, driven on
//! the calling thread, for callers without an event loop. The same engine runs underneath;
//! the worker still performs every USB call.

use std::fmt;
use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mio::{Events, Poll, Registry, Token, Waker};

use super::super::UsbBus;
use super::{NativeEvent, NativeUsb};
use crate::error::Error;
use crate::lowlevel::BusError;

/// The device's token on the managed reactor.
const DEVICE: Token = Token(0);
/// The worker's shared Waker on the managed reactor.
const WAKE: Token = Token(1);

/// A [`NativeUsb`] on a reactor of its own (feature `mio`), driven on whichever thread
/// calls [`next_event`](Self::next_event): the opt-in managed synchronous driver over the
/// native engine.
///
/// Start operations through [`native`](Self::native), then call
/// [`next_event`](Self::next_event) for their events and any RX blocks. Only the calling
/// thread waits, in the reactor; cancellation and cleanup still run on the handle's worker.
/// Unstable and unqualified on hardware, like [`NativeUsb`].
pub struct ManagedUsb {
    /// The handle; declared first, so it deregisters before its reactor is dropped.
    native: NativeUsb,
    /// The reactor only this runner drives.
    poll: Poll,
    /// The reactor's event buffer.
    events: Events,
}

impl fmt::Debug for ManagedUsb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ManagedUsb")
            .field("native", &self.native)
            .finish_non_exhaustive()
    }
}

impl ManagedUsb {
    /// Hands `bus` to a [`NativeUsb`] on a new reactor of its own, as
    /// [`UsbBus::into_native`] does; this blocks the same way.
    ///
    /// # Errors
    ///
    /// [`Error::Open`] if the reactor cannot be created; otherwise as
    /// [`UsbBus::into_native`].
    pub fn new(bus: UsbBus) -> Result<Self, Error> {
        Self::start(|registry, token, waker| bus.into_native(registry, token, waker))
    }

    /// Creates the reactor and its Waker, then attaches the handle `attach` makes.
    pub(in super::super) fn start(
        attach: impl FnOnce(&Registry, Token, Arc<Waker>) -> Result<NativeUsb, Error>,
    ) -> Result<Self, Error> {
        let poll = Poll::new().map_err(|error| Error::Open(error.into()))?;
        let waker = Waker::new(poll.registry(), WAKE).map_err(|error| Error::Open(error.into()))?;
        let native = attach(poll.registry(), DEVICE, Arc::new(waker))?;
        Ok(Self {
            native,
            poll,
            events: Events::with_capacity(8),
        })
    }

    /// The handle, to start operations on.
    pub fn native(&mut self) -> &mut NativeUsb {
        &mut self.native
    }

    /// Drives the reactor on this thread until the handle reports an event, honouring its
    /// deadlines; `None` once `timeout` passes without one.
    ///
    /// # Errors
    ///
    /// The reactor's own failure to poll.
    pub fn next_event(&mut self, timeout: Duration) -> io::Result<Option<NativeEvent>> {
        let until = Instant::now().checked_add(timeout);
        loop {
            let now = Instant::now();
            if let Some(event) = self.native.poll(now) {
                return Ok(Some(event));
            }
            if until.is_some_and(|until| now >= until) {
                return Ok(None);
            }
            let wake = match (self.native.deadline(), until) {
                (Some(deadline), Some(until)) => Some(deadline.min(until)),
                (deadline, until) => deadline.or(until),
            };
            let wait = wake.map(|wake| wake.saturating_duration_since(now));
            self.wait(wait)?;
            for event in &self.events {
                self.native.on_ready(event.token());
            }
        }
    }

    /// Closes the handle and waits up to `timeout` for the worker to cancel every
    /// transfer and release the device.
    ///
    /// # Errors
    ///
    /// The deregistration or reactor failure; the cleanup's failure; [`BusError::Timeout`]
    /// if cleanup has not finished in time (the worker carries on, detached).
    pub fn close(mut self, timeout: Duration) -> Result<(), BusError> {
        self.native.close()?;
        let until = Instant::now().checked_add(timeout);
        loop {
            if let Some(result) = self.native.try_cleanup() {
                return result;
            }
            let now = Instant::now();
            if until.is_some_and(|until| now >= until) {
                return Err(BusError::Timeout);
            }
            let wait = until.map(|until| until.saturating_duration_since(now));
            self.wait(wait)?;
        }
    }

    /// Waits in the reactor up to `timeout`; a signal's interruption is an empty wait, so
    /// the caller's loop simply looks again.
    fn wait(&mut self, timeout: Option<Duration>) -> io::Result<()> {
        match self.poll.poll(&mut self.events, timeout) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                self.events.clear();
                Ok(())
            }
            polled => polled,
        }
    }
}
