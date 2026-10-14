//! The managed asynchronous adapter: a [`NativeUsb`] on a reactor thread of its own,
//! observed through futures, for async callers. It needs no async runtime: the futures
//! are woken through `core::task::Waker`, the reactor thread through the handle's Mio
//! [`Waker`]. The worker still performs every USB call.

use std::collections::VecDeque;
use std::fmt;
use std::future::Future;
use std::io;
use std::mem;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll as Ready, Waker as TaskWaker};
use std::thread;
use std::time::{Duration, Instant};

use mio::{Events, Poll, Registry, Token, Waker};

use super::super::UsbBus;
use super::{NativeEvent, NativeUsb, RxBlock};
use crate::device::Device;
use crate::error::Error;
use crate::lowlevel::BusError;

/// The device's token on the reactor thread.
const DEVICE: Token = Token(0);
/// The shared Waker's token: the worker's reports and this adapter's commands.
const WAKE: Token = Token(1);
/// RX blocks held for the caller while no operation is in flight; beyond them the reactor
/// stops taking blocks, so the handle's own backlog fills and the gateware drops.
const BLOCKS: usize = 8;
/// How long the reactor thread waits for the worker's cleanup after closing.
const CLEANUP: Duration = Duration::from_secs(5);

/// An operation's start on the reactor thread.
type Start = Box<dyn FnOnce(&mut NativeUsb, Instant) -> Result<(), Error> + Send>;

/// Where the one operation is.
enum Slot {
    /// None in flight.
    Idle,
    /// Its start awaits the reactor thread.
    Queued(Start),
    /// Started; its event is awaited.
    Running,
    /// Ended: its event, or the refusal of its start; not yet taken by its future.
    Done(Result<NativeEvent, Error>),
}

/// What the caller's futures and the reactor thread share.
struct State {
    /// The operation.
    slot: Slot,
    /// Counts the operations started, so a future takes only its own operation's end.
    generation: u64,
    /// The operation's future, to wake once it ends.
    operation: Option<TaskWaker>,
    /// RX blocks received, oldest first.
    blocks: VecDeque<Result<RxBlock, Error>>,
    /// The block future, to wake once one arrives or the session ends.
    block: Option<TaskWaker>,
    /// Closing was asked for.
    closing: bool,
    /// The session ended: by failure, or once closed.
    ended: bool,
    /// The worker's cleanup result, once known.
    cleanup: Option<Result<(), BusError>>,
    /// The close future, to wake once cleanup is known.
    close: Option<TaskWaker>,
}

/// The shared state and the reactor thread's Waker.
struct Shared {
    /// The state.
    state: Mutex<State>,
    /// Wakes the reactor thread for a new command or freed room.
    reactor: Arc<Waker>,
}

impl Shared {
    /// The state, whatever a panicking holder left.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Wakes the reactor thread; a failed wake leaves it to its next event.
    fn nudge(&self) {
        let _ = self.reactor.wake();
    }
}

/// A [`NativeUsb`] on a reactor thread of its own (feature `mio`), observed through
/// futures: the opt-in managed asynchronous driver over the native engine, with no async
/// runtime dependency.
///
/// Start an operation with [`operation`](Self::operation), whose future resolves with its
/// event, and await RX blocks with [`next_block`](Self::next_block). One operation is in
/// flight at a time, as on the handle. The reactor thread owns the handle and its reactor;
/// [`close`](Self::close) ends it after the worker's cleanup. Dropping the adapter closes
/// it without waiting. Unstable and unqualified on hardware, like [`NativeUsb`].
pub struct AsyncUsb {
    /// What the futures and the reactor thread share.
    shared: Arc<Shared>,
}

impl fmt::Debug for AsyncUsb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = self.shared.lock();
        f.debug_struct("AsyncUsb")
            .field("ended", &state.ended)
            .field("blocks", &state.blocks.len())
            .finish_non_exhaustive()
    }
}

impl AsyncUsb {
    /// Hands `bus` to a [`NativeUsb`] on a new reactor thread, as
    /// [`UsbBus::into_native`] does; this blocks the same way, on the calling thread.
    ///
    /// # Errors
    ///
    /// [`Error::Open`] if the reactor or its thread cannot be made; otherwise as
    /// [`UsbBus::into_native`].
    pub fn new(bus: UsbBus) -> Result<Self, Error> {
        Self::start(|registry, token, waker| bus.into_native(registry, token, waker))
    }

    /// Hands a powered USB `device` to a [`NativeUsb`] on a new reactor thread, as
    /// [`Device::into_native`] does.
    ///
    /// # Errors
    ///
    /// [`Error::Open`] if the reactor or its thread cannot be made; otherwise as
    /// [`Device::into_native`].
    pub fn from_device(device: Device) -> Result<Self, Error> {
        Self::start(|registry, token, waker| device.into_native(registry, token, waker))
    }

    /// Creates the reactor and its Waker, attaches the handle `attach` makes, and moves
    /// both to the reactor thread.
    pub(in super::super) fn start(
        attach: impl FnOnce(&Registry, Token, Arc<Waker>) -> Result<NativeUsb, Error>,
    ) -> Result<Self, Error> {
        let poll = Poll::new().map_err(|error| Error::Open(error.into()))?;
        let waker = Waker::new(poll.registry(), WAKE).map_err(|error| Error::Open(error.into()))?;
        let waker = Arc::new(waker);
        let native = attach(poll.registry(), DEVICE, Arc::clone(&waker))?;
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                slot: Slot::Idle,
                generation: 0,
                operation: None,
                blocks: VecDeque::new(),
                block: None,
                closing: false,
                ended: false,
                cleanup: None,
                close: None,
            }),
            reactor: waker,
        });
        let reactor = Reactor {
            native,
            poll,
            events: Events::with_capacity(8),
            shared: Arc::clone(&shared),
        };
        // Detached: the thread ends after the worker's cleanup, or its bound.
        thread::Builder::new()
            .name("usdr-reactor".into())
            .spawn(move || reactor.run())
            .map_err(|error| Error::Open(error.into()))?;
        Ok(Self { shared })
    }

    /// Starts an operation on the reactor thread: `start` calls one of the handle's
    /// `start_*` methods with the thread's `now`. The future resolves with the operation's
    /// event (the first besides RX blocks), or with `start`'s refusal; [`Error::Busy`] at
    /// once while another operation is in flight, [`Error::Stream`] once the session ended.
    ///
    /// The operation starts when this is called, not when the future is first polled;
    /// dropping the future does not cancel it, and the next operation waits for it.
    pub fn operation(
        &self,
        start: impl FnOnce(&mut NativeUsb, Instant) -> Result<(), Error> + Send + 'static,
    ) -> Operation {
        let mut state = self.shared.lock();
        let refused = if state.ended || state.closing {
            Some(Error::Stream(BusError::Disconnected))
        } else if matches!(state.slot, Slot::Queued(_) | Slot::Running) {
            Some(Error::Busy)
        } else {
            None
        };
        if refused.is_none() {
            // An earlier operation's end its dropped future never took is discarded.
            state.slot = Slot::Queued(Box::new(start));
            state.generation += 1;
            state.operation = None;
        }
        let generation = state.generation;
        drop(state);
        if refused.is_none() {
            self.shared.nudge();
        }
        Operation {
            shared: Arc::clone(&self.shared),
            generation,
            refused: refused.map(Err),
        }
    }

    /// The next RX block, in arrival order; `None` once the session has ended and every
    /// block was taken.
    pub fn next_block(&self) -> NextBlock {
        NextBlock {
            shared: Arc::clone(&self.shared),
        }
    }

    /// Closes the handle on the reactor thread; resolves with the worker's cleanup (every
    /// transfer cancelled, the device released), or [`BusError::Timeout`] if it did not
    /// finish within five seconds (the worker carries on, detached).
    pub fn close(self) -> Closing {
        self.shared.lock().closing = true;
        self.shared.nudge();
        Closing {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl Drop for AsyncUsb {
    fn drop(&mut self) {
        self.shared.lock().closing = true;
        self.shared.nudge();
    }
}

/// An operation's future; see [`AsyncUsb::operation`].
#[must_use = "the operation runs anyway; its future reports how it ended"]
pub struct Operation {
    /// What the reactor thread fills in.
    shared: Arc<Shared>,
    /// The operation this future observes.
    generation: u64,
    /// The refusal before the operation started, taken once.
    refused: Option<Result<NativeEvent, Error>>,
}

impl fmt::Debug for Operation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Operation").finish_non_exhaustive()
    }
}

impl Future for Operation {
    type Output = Result<NativeEvent, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Ready<Self::Output> {
        let this = self.get_mut();
        if let Some(refused) = this.refused.take() {
            return Ready::Ready(refused);
        }
        let mut state = this.shared.lock();
        if state.generation != this.generation {
            // A later operation started: this one's end was discarded with its future's
            // interest, which only a dropped future gives up.
            return Ready::Ready(Err(Error::Busy));
        }
        match mem::replace(&mut state.slot, Slot::Idle) {
            Slot::Done(result) => Ready::Ready(result),
            slot => {
                state.slot = slot;
                state.operation = Some(cx.waker().clone());
                Ready::Pending
            }
        }
    }
}

/// An RX block's future; see [`AsyncUsb::next_block`].
#[must_use = "a block is taken only when the future is polled to completion"]
pub struct NextBlock {
    /// What the reactor thread fills in.
    shared: Arc<Shared>,
}

impl fmt::Debug for NextBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NextBlock").finish_non_exhaustive()
    }
}

impl Future for NextBlock {
    type Output = Option<Result<RxBlock, Error>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Ready<Self::Output> {
        let mut state = self.shared.lock();
        if let Some(block) = state.blocks.pop_front() {
            drop(state);
            // Room freed: the reactor may take more.
            self.shared.nudge();
            return Ready::Ready(Some(block));
        }
        if state.ended {
            return Ready::Ready(None);
        }
        state.block = Some(cx.waker().clone());
        Ready::Pending
    }
}

/// The close's future; see [`AsyncUsb::close`].
#[must_use = "closing proceeds anyway; its future reports the cleanup"]
pub struct Closing {
    /// What the reactor thread fills in.
    shared: Arc<Shared>,
}

impl fmt::Debug for Closing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Closing").finish_non_exhaustive()
    }
}

impl Future for Closing {
    type Output = Result<(), BusError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Ready<Self::Output> {
        let mut state = self.shared.lock();
        if let Some(result) = state.cleanup.take() {
            return Ready::Ready(result);
        }
        state.close = Some(cx.waker().clone());
        Ready::Pending
    }
}

/// The reactor thread's half: the handle, its reactor and the shared state.
struct Reactor {
    /// The handle; declared first, so it deregisters before its reactor is dropped.
    native: NativeUsb,
    /// The reactor.
    poll: Poll,
    /// Its event buffer.
    events: Events,
    /// What the caller's futures observe.
    shared: Arc<Shared>,
}

impl Reactor {
    /// Serves the shared state until closed, then awaits the worker's cleanup.
    fn run(mut self) {
        loop {
            let now = Instant::now();
            if self.serve(now) {
                break;
            }
            let wait = self
                .native
                .deadline()
                .map(|deadline| deadline.saturating_duration_since(now));
            if self.wait(wait).is_err() {
                self.end(Some(Error::Stream(BusError::Disconnected)));
                break;
            }
            for event in &self.events {
                self.native.on_ready(event.token());
            }
        }
        let cleanup = self.cleanup();
        let mut state = self.shared.lock();
        state.ended = true;
        state.cleanup = Some(cleanup);
        let wakers = [
            state.close.take(),
            state.block.take(),
            state.operation.take(),
        ];
        drop(state);
        wakers.into_iter().flatten().for_each(TaskWaker::wake);
    }

    /// Starts a queued operation and routes the handle's events, waking the futures they
    /// finish once the state is unlocked (an executor may poll inline on a wake); `true`
    /// once closing.
    fn serve(&mut self, now: Instant) -> bool {
        let mut woken = Vec::new();
        let closing = self.route(now, &mut woken);
        woken.into_iter().for_each(TaskWaker::wake);
        closing
    }

    /// [`Self::serve`]'s work, collecting the futures to wake in `woken`.
    fn route(&mut self, now: Instant, woken: &mut Vec<TaskWaker>) -> bool {
        let mut state = self.shared.lock();
        if state.closing {
            return true;
        }
        if let Slot::Queued(_) = state.slot {
            let Slot::Queued(start) = mem::replace(&mut state.slot, Slot::Running) else {
                unreachable!("invariant: matched as queued");
            };
            drop(state);
            let started = start(&mut self.native, now);
            state = self.shared.lock();
            if let Err(error) = started {
                state.slot = Slot::Done(Err(error));
                woken.extend(state.operation.take());
            }
        }
        // While no operation runs, a full block queue leaves blocks with the handle.
        while matches!(state.slot, Slot::Running) || state.blocks.len() < BLOCKS {
            drop(state);
            let event = self.native.poll(Instant::now());
            state = self.shared.lock();
            let Some(event) = event else {
                break;
            };
            match event {
                NativeEvent::Received(block) => {
                    state.blocks.push_back(block);
                    woken.extend(state.block.take());
                }
                NativeEvent::SessionEnded(error) => {
                    drop(state);
                    self.end(Some(Error::Stream(error)));
                    state = self.shared.lock();
                }
                event => {
                    if matches!(state.slot, Slot::Running) {
                        state.slot = Slot::Done(Ok(event));
                        woken.extend(state.operation.take());
                    }
                }
            }
        }
        // A failure an operation's event carried also ended the session.
        let ended = !state.ended && self.native.is_closed();
        drop(state);
        if ended {
            self.end(Some(Error::Stream(BusError::Disconnected)));
        }
        false
    }

    /// Marks the session ended: a running operation fails with `error`, and the block
    /// future sees the end.
    fn end(&self, error: Option<Error>) {
        let mut state = self.shared.lock();
        state.ended = true;
        let mut operation = None;
        if let (Slot::Running, Some(error)) = (&state.slot, error) {
            state.slot = Slot::Done(Err(error));
            operation = state.operation.take();
        }
        let block = state.block.take();
        drop(state);
        [operation, block]
            .into_iter()
            .flatten()
            .for_each(TaskWaker::wake);
    }

    /// Closes the handle and waits, in the reactor, for the worker's cleanup.
    fn cleanup(&mut self) -> Result<(), BusError> {
        self.native.close()?;
        let until = Instant::now() + CLEANUP;
        loop {
            if let Some(result) = self.native.try_cleanup() {
                return result;
            }
            let now = Instant::now();
            if now >= until {
                return Err(BusError::Timeout);
            }
            self.wait(Some(until - now))?;
        }
    }

    /// Waits in the reactor up to `timeout`; a signal's interruption is an empty wait.
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
