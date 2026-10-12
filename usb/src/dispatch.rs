//! Collecting completed transfers and handing each to its endpoint's queue.
//!
//! The kernel reports completions per open device, not per endpoint: whichever thread asks
//! gets the next one, whatever endpoint it belongs to. So one waiting thread at a time takes
//! the reaper role, collects everything that has completed, files each transfer under its
//! endpoint and wakes the others; the rest wait on a condition variable. Filing and waking
//! happen under one lock, so a waiter cannot miss its wakeup.

use std::collections::{HashMap, HashSet, VecDeque};
use std::num::NonZeroUsize;
use std::ptr::NonNull;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError, TryLockError};
use std::time::{Duration, Instant};

use rustix::io::Errno;

use crate::buffer::{Buffer, Mapping};
use crate::error::Error;
use crate::usbfs::Urb;

/// Whether a bounded reap observed an empty completion source or used its whole budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum ReapProgress {
    /// The backend reported no more completions at this instant. Completions already filed
    /// still need to be taken from each queue, and more can arrive immediately afterward.
    Drained,

    /// The budget was used without observing an empty source. Schedule another bounded
    /// turn before waiting for readiness; this does not prove another completion exists.
    BudgetExhausted,
}

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

    /// Asks the kernel to cancel `transfer`; ownership still returns only through reap.
    /// This call can block, even on a platform with non-waiting submission and reaping.
    ///
    /// # Safety
    ///
    /// `transfer` must have been submitted through this backend and must remain allocated
    /// for the call. It may already have completed or been reaped, but its owning queue
    /// must not have taken it back. A raced discard uses only the submitted address as an
    /// identity, never dereferencing reaped memory; the backend may refuse it with EINVAL.
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
    /// Files one transfer after the backend has returned ownership.
    fn push(&mut self, transfer: NonNull<Transfer>) {
        // SAFETY: reaped, so the kernel no longer touches it, and its queue does not until
        // it takes it from `done`; reading its endpoint is the only access.
        let endpoint = unsafe { transfer.as_ref() }.urb.endpoint;
        self.done
            .entry(endpoint)
            .or_default()
            .push_back(Reaped(transfer));
    }

    /// Files reaped transfers under their endpoints, or records the failure.
    fn file(&mut self, reaped: Result<Vec<NonNull<Transfer>>, Errno>) -> Result<(), Error> {
        match reaped {
            Ok(transfers) => {
                for transfer in transfers {
                    self.push(transfer);
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

    /// Collects at most `budget` completions, without waiting for readiness or the reaper
    /// role. Every collected prefix stays filed even if a later reap fails.
    pub(crate) fn reap(&self, budget: NonZeroUsize) -> Result<ReapProgress, Error> {
        let mut state = self.try_lock()?;
        if state.reaping {
            return Err(Error::Busy);
        }
        if state.disconnected {
            return Err(Error::Disconnected);
        }
        // Holding the state lock excludes another reaper for this bounded turn. Unlike
        // `wait`, this path never calls `wait_ready` or waits on the condition variable.
        let outcome = (|| {
            for _ in 0..budget.get() {
                match self.backend.reap() {
                    Ok(Some(transfer)) => state.push(transfer),
                    Ok(None) => return Ok(ReapProgress::Drained),
                    Err(errno @ (Errno::NODEV | Errno::SHUTDOWN)) => {
                        state.disconnected = true;
                        return Err(errno.into());
                    }
                    Err(errno) => return Err(errno.into()),
                }
            }
            Ok(ReapProgress::BudgetExhausted)
        })();
        self.filed.notify_all();
        outcome
    }

    /// Takes only a filed completion, without backend calls or waiting for the state lock.
    pub(crate) fn try_complete(&self, endpoint: u8) -> Result<Option<NonNull<Transfer>>, Error> {
        let mut state = self.try_lock()?;
        if let Some(Reaped(transfer)) = state.done.get_mut(&endpoint).and_then(VecDeque::pop_front)
        {
            return Ok(Some(transfer));
        }
        if state.disconnected {
            return Err(Error::Disconnected);
        }
        Ok(None)
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

    /// The state without waiting for another thread, recovering consistent poisoned state.
    fn try_lock(&self) -> Result<MutexGuard<'_, State>, Error> {
        match self.state.try_lock() {
            Ok(state) => Ok(state),
            Err(TryLockError::Poisoned(error)) => Ok(error.into_inner()),
            Err(TryLockError::WouldBlock) => Err(Error::Busy),
        }
    }
}

#[cfg(test)]
pub(crate) mod fake {
    //! A backend without a kernel: tests complete transfers by hand.

    use std::sync::{Arc, Barrier, Condvar, Mutex};

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
        /// Reap and readiness-wait calls, for their bounded/non-waiting contracts.
        calls: (usize, usize),
        /// A single reap error after the given number of calls.
        reap_error: Option<(usize, Errno)>,
        /// A single discard error after the given number of calls.
        discard_error: Option<(usize, Errno)>,
        /// Pauses the next reap after ownership leaves the fake kernel.
        reap_pause: Option<Arc<Pause>>,
        /// Pauses the next discard before cancellation finishes.
        discard_pause: Option<Arc<Pause>>,
        /// Pauses the next readiness wait after the dispatcher takes the reaper role.
        wait_pause: Option<Arc<Pause>>,
    }

    /// A deterministic boundary pause: no sleeps or scheduler assumptions.
    #[derive(Debug)]
    pub(crate) struct Pause {
        entered: Barrier,
        released: Barrier,
    }

    impl Pause {
        /// One backend caller and one test coordinator meet at each barrier.
        fn new() -> Arc<Self> {
            Arc::new(Self {
                entered: Barrier::new(2),
                released: Barrier::new(2),
            })
        }

        /// Waits for the backend to reach the boundary.
        pub(crate) fn entered(&self) {
            self.entered.wait();
        }

        /// Lets the paused backend return.
        pub(crate) fn release(&self) {
            self.released.wait();
        }

        /// Pauses a backend call while the coordinator inspects ownership/progress.
        fn stop(&self) {
            self.entered();
            self.release();
        }
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

        /// Reap calls and readiness waits so far.
        pub(crate) fn calls(&self) -> (usize, usize) {
            self.state.lock().unwrap().calls
        }

        /// Injects a one-shot error after `calls` successful reap attempts.
        pub(crate) fn fail_reap_after(&self, calls: usize, errno: Errno) {
            self.state.lock().unwrap().reap_error = Some((calls, errno));
        }

        /// Injects a one-shot discard error, without returning the transfer's ownership.
        pub(crate) fn fail_discard_after(&self, calls: usize, errno: Errno) {
            self.state.lock().unwrap().discard_error = Some((calls, errno));
        }

        /// Holds the next reap before it returns its already-completed pointer.
        pub(crate) fn pause_reap(&self) -> Arc<Pause> {
            let pause = Pause::new();
            self.state.lock().unwrap().reap_pause = Some(Arc::clone(&pause));
            pause
        }

        /// Holds the next discard, modelling a potentially blocking cancellation ioctl.
        pub(crate) fn pause_discard(&self) -> Arc<Pause> {
            let pause = Pause::new();
            self.state.lock().unwrap().discard_pause = Some(Arc::clone(&pause));
            pause
        }

        /// Holds the next readiness wait without holding the fake kernel's state lock.
        pub(crate) fn pause_wait(&self) -> Arc<Pause> {
            let pause = Pause::new();
            self.state.lock().unwrap().wait_pause = Some(Arc::clone(&pause));
            pause
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
            let pause = self.state.lock().unwrap().discard_pause.take();
            if let Some(pause) = pause {
                pause.stop();
            }
            let mut state = self.state.lock().unwrap();
            state.discarded += 1;
            if let Some((calls, errno)) = state.discard_error.as_mut() {
                if *calls == 0 {
                    let errno = *errno;
                    state.discard_error = None;
                    return Err(errno);
                }
                *calls -= 1;
            }
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
            state.calls.0 += 1;
            if let Some((calls, errno)) = state.reap_error.as_mut() {
                if *calls == 0 {
                    let errno = *errno;
                    state.reap_error = None;
                    return Err(errno);
                }
                *calls -= 1;
            }
            let outcome = match state.completed.pop_front() {
                Some(Reaped(transfer)) => Ok(Some(transfer)),
                None if state.unplugged => Err(Errno::NODEV),
                None => Ok(None),
            };
            let pause = state.reap_pause.take();
            drop(state);
            if let Some(pause) = pause {
                pause.stop();
            }
            outcome
        }

        fn wait_ready(&self, timeout: Duration) -> Result<(), Errno> {
            let pause = {
                let mut state = self.state.lock().unwrap();
                state.calls.1 += 1;
                state.wait_pause.take()
            };
            if let Some(pause) = pause {
                pause.stop();
            }
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
