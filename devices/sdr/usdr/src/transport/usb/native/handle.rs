//! The public reactor-driven handle over the native adapter; TMP114 temperature reads only.

use std::fmt;
use std::io;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mio::{Registry, Token, Waker};

use super::super::i2c::{Action, I2cTransaction};
use super::super::protocol::I2cCache;
use super::adapter::Adapter;
use super::backend::Backend;
use crate::chips::tmp114::{TemperatureRead, Tmp114};
use crate::error::Error;
use crate::lowlevel::{BusError, I2cAddr};

/// Completions the worker files per turn; the handle keeps one request in flight.
const BUDGET: NonZeroUsize = NonZeroUsize::MIN.saturating_add(7);

/// Control-plane state carried over from the blocking bus, so the native session continues
/// its notification sequence baseline, I²C address table and operation identities.
pub(in super::super) struct Session {
    /// The simple I²C core's address table.
    pub i2c: I2cCache,
    /// Expected next global notification number.
    pub next_sequence: Option<u16>,
    /// Never-reused identity for the next I²C operation.
    pub next_id: u64,
}

/// Something a [`NativeUsb`] finished.
#[derive(Debug)]
#[non_exhaustive]
pub enum NativeEvent {
    /// A temperature read finished, in °C. A failure also closes the handle.
    Temperature(Result<f32, Error>),
}

/// A uSDR's USB control plane driven by a caller-owned Mio reactor (feature `mio`), made by
/// [`UsbBus::into_native`](super::super::UsbBus::into_native).
///
/// Unstable pilot: it reads the TMP114 temperature and nothing else. It powers nothing, so it
/// applies no thermal policy. The reactor thread only registers, schedules and observes; a
/// worker thread owns the USB queues and performs every USB call, including cleanup.
///
/// Drive it from your event loop: pass every event for its token to
/// [`on_ready`](Self::on_ready), then call [`poll`](Self::poll) after any event for its
/// token or the shared [`Waker`], and once [`deadline`](Self::deadline) passes. Pass a
/// `now` that never decreases across calls; a step backwards fails the read. One read is
/// in flight at a time. Any failure other than refusing a second read closes the handle;
/// open the device again to continue. Unqualified on hardware.
pub struct NativeUsb {
    /// Registration, deadlines and the worker's mailboxes.
    adapter: Adapter,
    /// Carried over from the blocking bus.
    session: Session,
    /// Origin of the machine's caller-supplied time.
    epoch: Instant,
    /// The pending read and its I²C transaction.
    reading: Option<(TemperatureRead, I2cTransaction)>,
    /// A worker handoff failure noticed outside `poll`, reported by the next `poll`.
    broken: Option<BusError>,
}

impl fmt::Debug for NativeUsb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeUsb")
            .field("reading", &self.reading.is_some())
            .finish_non_exhaustive()
    }
}

impl NativeUsb {
    /// Attaches `backend` to `registry` under `token`, continuing `session`.
    pub(in super::super) fn attach<B: Backend>(
        backend: B,
        session: Session,
        registry: &Registry,
        token: Token,
        waker: Arc<Waker>,
    ) -> Result<Self, BusError> {
        Ok(Self {
            adapter: Adapter::attach(backend, registry, token, waker, BUDGET)?,
            session,
            epoch: Instant::now(),
            reading: None,
            broken: None,
        })
    }

    /// Starts reading the TMP114 temperature; [`poll`](Self::poll) reports the result.
    ///
    /// # Errors
    ///
    /// [`Error::Bus`] if a read is already pending, the handle is closed, or the command
    /// cannot be handed to the worker; all but the first close the handle.
    pub fn start_temperature(&mut self, now: Instant) -> Result<(), Error> {
        let (mut operation, request) = TemperatureRead::begin(Tmp114::usdr());
        if self.reading.is_some() {
            let pending = BusError::Unsupported("a native temperature read is pending");
            return Err(operation.fail(pending));
        }
        match self.begin(request.dev, &request.register, now) {
            Ok(transaction) => {
                self.reading = Some((operation, transaction));
                Ok(())
            }
            Err(error) => {
                self.adapter.retire_after_completion();
                Err(operation.fail(error))
            }
        }
    }

    /// Prepares and submits a read's first command.
    fn begin(
        &mut self,
        dev: I2cAddr,
        register: &[u8],
        now: Instant,
    ) -> Result<I2cTransaction, BusError> {
        let id = self.session.next_id;
        let next = id.checked_add(1).ok_or(BusError::Unsupported(
            "USB I2C operation identity exhausted; reopen it",
        ))?;
        let elapsed = self.elapsed(now);
        let mut transaction =
            I2cTransaction::new(id, dev, register, 2, &mut self.session.i2c, elapsed)?;
        self.session.next_id = next;
        let action = transaction.start(elapsed)?;
        self.submit(&mut transaction, action, now)?;
        Ok(transaction)
    }

    /// Forwards one Mio event's token; events for other tokens are ignored.
    pub fn on_ready(&mut self, token: Token) {
        if let Err(error) = self.adapter.on_ready(token) {
            self.broken.get_or_insert(error);
        }
    }

    /// Observes the worker and deadline once; never waits or performs USB I/O.
    #[must_use]
    pub fn poll(&mut self, now: Instant) -> Option<NativeEvent> {
        let (mut operation, mut transaction) = self.reading.take()?;
        let advanced = match self.broken.take() {
            Some(error) => Err(error),
            None => self.advance(&mut transaction, now),
        };
        let result = match advanced {
            Ok(None) => {
                self.reading = Some((operation, transaction));
                return None;
            }
            Ok(Some((bytes, len))) => operation.resolve(Ok(&bytes[..len])),
            Err(error) => operation.resolve(Err(error)),
        };
        if result.is_err() {
            // Malformed data and local failures end the session too, not just transport ones.
            self.adapter.retire_after_completion();
        }
        Some(NativeEvent::Temperature(result))
    }

    /// Applies at most one worker result and submits the transaction's next command.
    fn advance(
        &mut self,
        transaction: &mut I2cTransaction,
        now: Instant,
    ) -> Result<Option<([u8; 4], usize)>, BusError> {
        let Some(completion) = self.adapter.try_event(now)? else {
            return Ok(None);
        };
        let elapsed = self.elapsed(now);
        match completion.apply(transaction, elapsed, &mut self.session.next_sequence)? {
            Action::Complete { bytes, len } => Ok(Some((bytes, len))),
            action => {
                self.submit(transaction, action, now)?;
                Ok(None)
            }
        }
    }

    /// Refreshes `action`'s timeout and hands it to the worker.
    fn submit(
        &mut self,
        transaction: &mut I2cTransaction,
        action: Action,
        now: Instant,
    ) -> Result<(), BusError> {
        let action = transaction.dispatch(action, self.elapsed(now))?;
        self.adapter.submit(&action, now)
    }

    /// The machine's time for `now`.
    fn elapsed(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.epoch)
    }

    /// When [`poll`](Self::poll) must run even without an event.
    #[must_use]
    pub fn deadline(&self) -> Option<Instant> {
        self.adapter.deadline()
    }

    /// Deregisters and lets the worker cancel, release the device and report through
    /// [`try_cleanup`](Self::try_cleanup); never waits.
    ///
    /// # Errors
    ///
    /// The deregistration failure. `NotFound` has still retired the worker; after any other
    /// error the token stays registered and `close` may be retried.
    pub fn close(&mut self) -> io::Result<()> {
        self.reading = None;
        self.adapter.close()
    }

    /// The worker's cleanup result, once, after [`close`](Self::close) or a failure.
    pub fn try_cleanup(&mut self) -> Option<Result<(), BusError>> {
        self.adapter.try_cleanup()
    }

    /// A deregistration error from a failure's automatic close, once.
    pub fn take_retirement_error(&mut self) -> Option<io::Error> {
        self.adapter.take_retirement_error()
    }
}
