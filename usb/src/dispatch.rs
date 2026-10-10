//! Collecting completed transfers and handing each to its endpoint's queue.
//!
//! The kernel reports completions per open device, not per endpoint: whichever thread asks
//! gets the next one, whatever endpoint it belongs to. So one waiting thread at a time takes
//! the reaper role, collects everything that has completed, files each transfer under its
//! endpoint and wakes the others; the rest wait on a condition variable. Filing and waking
//! happen under one lock, so a waiter cannot miss its wakeup.

use std::collections::{HashMap, HashSet, VecDeque};
use std::ptr::NonNull;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use rustix::io::Errno;

use crate::buffer::{Buffer, Mapping};
use crate::error::Error;
use crate::usbfs::Urb;

/// A transfer in flight: what the kernel reads and writes, and the buffer it moves data
/// through. Boxed, and owned by the kernel from submission until it is reaped.
#[repr(C)]
#[derive(Debug)]
pub(crate) struct Transfer {
    /// The kernel's view; first, so a pointer to the transfer is a pointer to it.
    pub(crate) urb: Urb,

    /// The memory `urb` points into.
    pub(crate) buffer: Buffer,
}

/// What the dispatcher needs from a platform.
pub(crate) trait Backend: Send + Sync {
    /// Hands `transfer` to the kernel.
    ///
    /// # Safety
    ///
    /// `transfer` must stay allocated, and nothing may touch it or its buffer, until
    /// [`Backend::reap`] returns it.
    unsafe fn submit(&self, transfer: NonNull<Transfer>) -> Result<(), Errno>;

    /// Asks the kernel to cancel `transfer`; it is still reaped, with a cancelled status.
    ///
    /// # Safety
    ///
    /// `transfer` must have been submitted and not yet reaped.
    unsafe fn discard(&self, transfer: NonNull<Transfer>) -> Result<(), Errno>;

    /// A completed transfer, if any, without waiting.
    fn reap(&self) -> Result<Option<NonNull<Transfer>>, Errno>;

    /// Waits up to `timeout` for a transfer to complete.
    fn wait_ready(&self, timeout: Duration) -> Result<(), Errno>;

    /// Device memory for a transfer buffer of `len` bytes, if the platform can map it.
    fn map(&self, len: usize) -> Option<Mapping>;
}

/// A shared backend: the device's node is shared by the dispatcher and its handles.
impl<B: Backend> Backend for std::sync::Arc<B> {
    unsafe fn submit(&self, transfer: NonNull<Transfer>) -> Result<(), Errno> {
        // SAFETY: the caller upholds `Backend::submit`.
        unsafe { B::submit(self, transfer) }
    }

    unsafe fn discard(&self, transfer: NonNull<Transfer>) -> Result<(), Errno> {
        // SAFETY: the caller upholds `Backend::discard`.
        unsafe { B::discard(self, transfer) }
    }

    fn reap(&self) -> Result<Option<NonNull<Transfer>>, Errno> {
        B::reap(self)
    }

    fn wait_ready(&self, timeout: Duration) -> Result<(), Errno> {
        B::wait_ready(self, timeout)
    }

    fn map(&self, len: usize) -> Option<Mapping> {
        B::map(self, len)
    }
}

/// A reaped transfer awaiting its queue.
#[derive(Debug)]
struct Reaped(NonNull<Transfer>);

// SAFETY: a reaped transfer belongs to no thread: the kernel is done with it and only the
// queue that submitted it will take it out of the dispatcher.
unsafe impl Send for Reaped {}

/// One open device's completions.
pub(crate) struct Dispatcher {
    /// The platform.
    backend: Box<dyn Backend>,

    /// Filed completions and who is reaping.
    state: Mutex<State>,

    /// Signalled whenever completions are filed or the reaper role is released.
    filed: Condvar,
}

/// The dispatcher's shared state.
#[derive(Debug, Default)]
struct State {
    /// A thread holds the reaper role.
    reaping: bool,

    /// The device has gone; nothing more will complete.
    disconnected: bool,

    /// Reaped transfers per endpoint address, oldest first.
    done: HashMap<u8, VecDeque<Reaped>>,

    /// Endpoints with a queue.
    open: HashSet<u8>,
}

impl State {
    /// Files reaped transfers under their endpoints, or records the failure.
    fn file(&mut self, reaped: Result<Vec<NonNull<Transfer>>, Errno>) -> Result<(), Error> {
        match reaped {
            Ok(transfers) => {
                for transfer in transfers {
                    // SAFETY: reaped, so the kernel no longer touches it, and its queue does
                    // not until it takes it from `done`; reading its endpoint is the only
                    // access.
                    let endpoint = unsafe { transfer.as_ref() }.urb.endpoint;
                    self.done
                        .entry(endpoint)
                        .or_default()
                        .push_back(Reaped(transfer));
                }
                Ok(())
            }
            Err(Errno::NODEV | Errno::SHUTDOWN) => {
                self.disconnected = true;
                Ok(())
            }
            Err(errno) => Err(errno.into()),
        }
    }
}

impl std::fmt::Debug for Dispatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dispatcher").finish_non_exhaustive()
    }
}

impl Dispatcher {
    /// A dispatcher for one open device.
    pub(crate) fn new(backend: Box<dyn Backend>) -> Self {
        Self {
            backend,
            state: Mutex::new(State::default()),
            filed: Condvar::new(),
        }
    }

    /// The platform.
    pub(crate) fn backend(&self) -> &dyn Backend {
        self.backend.as_ref()
    }

    /// Registers a queue on `endpoint`; one at a time.
    pub(crate) fn open(&self, endpoint: u8) -> Result<(), Error> {
        if self.lock().open.insert(endpoint) {
            Ok(())
        } else {
            Err(Error::Busy)
        }
    }

    /// Unregisters `endpoint`'s queue, which must have nothing in flight.
    pub(crate) fn close(&self, endpoint: u8) {
        let mut state = self.lock();
        state.open.remove(&endpoint);
        state.done.remove(&endpoint);
    }

    /// The next completed transfer on `endpoint`, waiting until `deadline`; `None` if none
    /// completed in time.
    ///
    /// # Errors
    ///
    /// [`Error::Disconnected`] once the device has gone and nothing remains filed for
    /// `endpoint`; any other failure to collect completions.
    pub(crate) fn wait(
        &self,
        endpoint: u8,
        deadline: Instant,
    ) -> Result<Option<NonNull<Transfer>>, Error> {
        let mut state = self.lock();
        // Even a wait that is already due collects what has completed, once.
        let mut collected = false;
        loop {
            if let Some(Reaped(transfer)) =
                state.done.get_mut(&endpoint).and_then(VecDeque::pop_front)
            {
                return Ok(Some(transfer));
            }
            if state.disconnected {
                return Err(Error::Disconnected);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() && (collected || state.reaping) {
                return Ok(None);
            }
            collected = true;
            if state.reaping {
                state = self
                    .filed
                    .wait_timeout(state, remaining)
                    .unwrap_or_else(PoisonError::into_inner)
                    .0;
                continue;
            }

            state.reaping = true;
            drop(state);
            let reaped = self.reap_some(remaining);
            state = self.lock();
            state.reaping = false;
            let outcome = state.file(reaped);
            self.filed.notify_all();
            outcome?;
        }
    }

    /// Collects every completed transfer, waiting up to `timeout` for the first.
    fn reap_some(&self, timeout: Duration) -> Result<Vec<NonNull<Transfer>>, Errno> {
        let mut reaped = self.drain()?;
        if reaped.is_empty() {
            self.backend.wait_ready(timeout)?;
            reaped = self.drain()?;
        }
        Ok(reaped)
    }

    /// Collects every transfer completed so far. A failure after some were collected is
    /// left for the next call, so none are lost.
    fn drain(&self) -> Result<Vec<NonNull<Transfer>>, Errno> {
        let mut reaped = Vec::new();
        loop {
            match self.backend.reap() {
                Ok(Some(transfer)) => reaped.push(transfer),
                Ok(None) => return Ok(reaped),
                Err(_) if !reaped.is_empty() => return Ok(reaped),
                Err(errno) => return Err(errno),
            }
        }
    }

    /// The state, even if a thread panicked holding it: every update leaves it consistent.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
pub(crate) mod fake {
    //! A backend without a kernel: tests complete transfers by hand.

    use std::sync::{Condvar, Mutex};

    use super::*;

    /// Submitted transfers and those the test has completed.
    #[derive(Debug, Default)]
    pub(crate) struct Fake {
        state: Mutex<FakeState>,
        changed: Condvar,
    }

    #[derive(Debug, Default)]
    struct FakeState {
        /// Submitted, not yet completed, in submission order.
        pending: Vec<Reaped>,
        /// Completed, not yet reaped.
        completed: VecDeque<Reaped>,
        /// The device has gone.
        unplugged: bool,
        /// Discard requests, by transfer.
        discarded: usize,
    }

    impl Fake {
        /// Completes the oldest pending transfer on `endpoint` with `status` and `bytes`.
        pub(crate) fn complete(&self, endpoint: u8, status: i32, bytes: usize) {
            let mut state = self.state.lock().unwrap();
            let index = state
                .pending
                .iter()
                // SAFETY: pending transfers are only read here, under the lock.
                .position(|Reaped(t)| unsafe { t.as_ref() }.urb.endpoint == endpoint)
                .expect("a transfer is pending on the endpoint");
            let Reaped(mut transfer) = state.pending.remove(index);
            // SAFETY: "the kernel" owns a pending transfer; this writes its outcome as usbfs
            // does at reap.
            let urb = &mut unsafe { transfer.as_mut() }.urb;
            urb.status = status;
            urb.actual_length = i32::try_from(bytes).unwrap();
            assert!(
                urb.actual_length <= urb.buffer_length,
                "the device sends at most the length"
            );
            // Like the kernel, write the arriving data through the pointer the transfer gave.
            if urb.endpoint & 0x80 != 0 {
                // SAFETY: `buffer` addresses `buffer_length` bytes the transfer owns.
                unsafe { std::ptr::write_bytes(urb.buffer.cast::<u8>(), endpoint, bytes) };
            }
            state.completed.push_back(Reaped(transfer));
            self.changed.notify_all();
        }

        /// The device goes away: everything pending completes with ESHUTDOWN, as usbfs does.
        pub(crate) fn unplug(&self) {
            let mut state = self.state.lock().unwrap();
            for Reaped(mut transfer) in std::mem::take(&mut state.pending) {
                // SAFETY: as in `complete`.
                unsafe { transfer.as_mut() }.urb.status = -Errno::SHUTDOWN.raw_os_error();
                state.completed.push_back(Reaped(transfer));
            }
            state.unplugged = true;
            self.changed.notify_all();
        }

        /// Transfers submitted and not completed.
        pub(crate) fn pending(&self) -> usize {
            self.state.lock().unwrap().pending.len()
        }

        /// Discard requests made.
        pub(crate) fn discarded(&self) -> usize {
            self.state.lock().unwrap().discarded
        }
    }

    impl Backend for Fake {
        unsafe fn submit(&self, transfer: NonNull<Transfer>) -> Result<(), Errno> {
            let mut state = self.state.lock().unwrap();
            if state.unplugged {
                return Err(Errno::NODEV);
            }
            state.pending.push(Reaped(transfer));
            Ok(())
        }

        unsafe fn discard(&self, transfer: NonNull<Transfer>) -> Result<(), Errno> {
            let mut state = self.state.lock().unwrap();
            state.discarded += 1;
            let Some(index) = state.pending.iter().position(|Reaped(t)| *t == transfer) else {
                return Err(Errno::INVAL);
            };
            let Reaped(mut transfer) = state.pending.remove(index);
            // SAFETY: as in `complete`.
            unsafe { transfer.as_mut() }.urb.status = -Errno::NOENT.raw_os_error();
            state.completed.push_back(Reaped(transfer));
            self.changed.notify_all();
            Ok(())
        }

        fn reap(&self) -> Result<Option<NonNull<Transfer>>, Errno> {
            let mut state = self.state.lock().unwrap();
            match state.completed.pop_front() {
                Some(Reaped(transfer)) => Ok(Some(transfer)),
                None if state.unplugged => Err(Errno::NODEV),
                None => Ok(None),
            }
        }

        fn wait_ready(&self, timeout: Duration) -> Result<(), Errno> {
            let state = self.state.lock().unwrap();
            let _ = self
                .changed
                .wait_timeout_while(state, timeout, |s| s.completed.is_empty() && !s.unplugged)
                .unwrap();
            Ok(())
        }

        fn map(&self, _len: usize) -> Option<Mapping> {
            None
        }
    }
}
