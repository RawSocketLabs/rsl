//! Private execution pilot: identical production machines behind different test runners.
//!
//! Mio's source is a shared Waker, not a USB FD. The owned thread, future and callback
//! adapters here are proofs, not shipped APIs or qualification of native nonblocking USB.
//! The actual Device temperature path uses both machines exercised here. RX uses the
//! existing production adapter and the independently modeled whole-transfer boundary.

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::task::{Context, Poll as TaskPoll, Wake, Waker as TaskWaker};
use std::thread;
use std::time::Duration;

use mio::{Events, Poll, Token, Waker};

use super::{BoardQueue, BoardRevision, BoardUsb, SimBoard, StreamScript};
use crate::chips::tmp114::{TemperatureRead, Tmp114};
use crate::error::Error;
use crate::lowlevel::BusError;
use crate::transport::usb::bus::{Reply, UsbIo};
use crate::transport::usb::i2c::{Action, Event, I2cTransaction};
use crate::transport::usb::protocol::I2cCache;
use crate::transport::usb::rx::Rx;

/// One bounded result cell; dropping an observer does not cancel an operation.
#[derive(Default)]
struct Completion {
    /// Registration and result publication use the same lock to prevent lost wakes.
    state: Mutex<CompletionState>,
    /// Only the blocking observer uses this condition variable.
    changed: Condvar,
}

/// Published exactly once, even when timeout, cancellation and late events race.
#[derive(Default)]
struct CompletionState {
    /// The single observer's value, retained until observed.
    result: Option<Result<f32, Error>>,
    /// A terminal result was published, independently of whether it was consumed.
    finished: bool,
    /// The async observer's latest wake registration.
    wake: Option<TaskWaker>,
}

impl Completion {
    /// Publishes before waking; the runtime never interprets board policy.
    fn resolve(&self, result: Result<f32, Error>) {
        let wake = {
            let mut state = self.state.lock().unwrap();
            assert!(
                !state.finished,
                "one terminal result per temperature operation"
            );
            state.finished = true;
            state.result = Some(result);
            state.wake.take()
        };
        self.changed.notify_all();
        if let Some(wake) = wake {
            wake.wake();
        }
    }
}

/// A blocking or future observer with no access to transport work.
struct Observer(Arc<Completion>);

impl Observer {
    /// Waits for a separately driven producer; never drives the reactor itself.
    fn wait(self) -> Result<f32, Error> {
        let mut state = self.0.state.lock().unwrap();
        loop {
            if let Some(result) = state.result.take() {
                return result;
            }
            state = self.0.changed.wait(state).unwrap();
        }
    }
}

impl Future for Observer {
    type Output = Result<f32, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> TaskPoll<Self::Output> {
        let mut state = self.0.state.lock().unwrap();
        if let Some(result) = state.result.take() {
            TaskPoll::Ready(result)
        } else {
            state.wake = Some(cx.waker().clone());
            TaskPoll::Pending
        }
    }
}

/// Owner of the pure chip/USB machines and the independently modeled endpoint boundary.
struct Pilot {
    /// Temperature interpretation used by `Device::temperature`, not duplicated in runners.
    temperature: TemperatureRead,
    /// Register-command and scalar-IRQ machine used by `UsbBus::i2c`.
    transaction: I2cTransaction,
    /// Next executor action; at most one control transfer is outstanding.
    action: Option<Action>,
    /// Session-owned notification sequence, shared across operation phases.
    sequence: Option<u16>,
    /// Whole-transfer emulator; owns any production RX adapter opened by the test.
    io: BoardUsb,
    /// Observers own only the result, not the machines or OS buffers.
    completion: Arc<Completion>,
}

impl Pilot {
    /// Begins one already-identified sensor read; startup policy is tested through Device.
    fn new(id: u64) -> (Self, Observer) {
        let (temperature, request) = TemperatureRead::begin(Tmp114::usdr());
        let mut transaction = I2cTransaction::new(
            id,
            request.dev,
            &request.register,
            2,
            &mut I2cCache::default(),
            Duration::ZERO,
        )
        .unwrap();
        let action = Some(transaction.start(Duration::ZERO).unwrap());
        let completion = Arc::new(Completion::default());
        let observer = Observer(Arc::clone(&completion));
        (
            Self {
                temperature,
                transaction,
                action,
                sequence: None,
                io: BoardUsb {
                    script: Arc::default(),
                    rx: None,
                    tx: None,
                    board: Arc::new(Mutex::new(SimBoard::new(BoardRevision::Rev3))),
                    registers: VecDeque::new(),
                    notifications: VecDeque::new(),
                    sequence: 0,
                    lut: 0,
                    index: 0,
                },
                completion,
            },
            observer,
        )
    }

    /// Accepts a correlated callback; terminal sensor results cannot be overwritten.
    fn on_event(&mut self, event: Event<'_>, now: Duration) {
        match self.transaction.complete(event, now, &mut self.sequence) {
            Ok(Action::Complete { bytes, len }) => self.finish(Ok(&bytes[..len])),
            Ok(next) => self.action = Some(next),
            Err(error) => self.finish(Err(error)),
        }
    }

    /// Drives deadlines even when an I/O source never reports another completion.
    fn advance(&mut self, now: Duration) {
        if let Err(error) = self.transaction.advance(now) {
            self.finish(Err(error));
        }
    }

    /// Executes one fake boundary action, used unchanged by every runner.
    fn step(&mut self, now: Duration) {
        let Some(action) = self.action.take() else {
            return;
        };
        let action = match self.transaction.dispatch(action, now) {
            Ok(action) => action,
            Err(error) => {
                self.finish(Err(error));
                return;
            }
        };
        match action {
            Action::Write {
                id,
                packet,
                timeout,
            } => match self.io.write(&packet, timeout) {
                Ok(()) => self.on_event(Event::Written { id }, now),
                Err(error) => self.on_event(Event::Failed { id, error }, now),
            },
            Action::Notification { id, timeout } => {
                match self.io.read(Reply::Notification, timeout) {
                    Ok(packet) => self.on_event(
                        Event::Notification {
                            id,
                            packet: &packet,
                        },
                        now,
                    ),
                    Err(error) => self.on_event(Event::Failed { id, error }, now),
                }
            }
            Action::Complete { .. } => panic!("terminal actions are delivered, never executed"),
        }
    }

    /// Resolves once and stops submission, leaving RX cleanup independently available.
    fn finish(&mut self, result: Result<&[u8], BusError>) {
        self.action = None;
        if let Some(result) = self.temperature.complete(result) {
            self.completion.resolve(result);
        }
    }

    /// Explicit issued-operation cancellation; the observer itself cannot request it.
    fn abort(&mut self) {
        let error = self.transaction.cancel(Duration::ZERO).unwrap_err();
        self.finish(Err(error));
    }

    /// Opens the real bounded RX adapter without pretending to configure an RF stream.
    fn open_rx(&mut self) -> Arc<Mutex<StreamScript>> {
        self.io.rx_open(8).unwrap();
        Arc::clone(&self.io.script)
    }
}

impl Drop for Pilot {
    fn drop(&mut self) {
        self.finish(Err(BusError::Disconnected));
        // This boundary has no kernel-owned memory; actual queue Drop remains unchanged.
        let _ = self.io.rx_close();
    }
}

/// Drives a fake source through a caller-owned Poll and its shared notifier.
fn drive_mio(pilot: &mut Pilot) {
    let mut poll = Poll::new().unwrap();
    let mut events = Events::with_capacity(8);
    let notifier = Arc::new(Waker::new(poll.registry(), Token(17)).unwrap());
    // Both sources share one Waker: one wake can cover multiple producers or no completion.
    let other_source = Arc::clone(&notifier);
    other_source.wake().unwrap();
    while pilot.action.is_some() {
        notifier.wake().unwrap();
        poll.poll(&mut events, Some(Duration::ZERO)).unwrap();
        assert!(
            !events.is_empty(),
            "the local fake source signalled readiness"
        );
        // Readiness is a hint, not a completion; bounded pending work is drained separately.
        for _ in 0..2 {
            pilot.step(Duration::ZERO);
        }
    }
}

/// Counted async notification; no executor or sleeps are needed for future tests.
#[derive(Default)]
struct CountWake(AtomicUsize);

impl Wake for CountWake {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// A pinned-format sample frame, not an RF or DMA timing model.
fn sample_frame() -> Vec<u8> {
    let mut frame = vec![0x5a; 8];
    frame.extend(3_u64.to_le_bytes());
    frame.extend(0_u64.to_le_bytes());
    frame
}

#[test]
fn callback_and_mio_runners_execute_the_same_temperature_and_irq_sequence() {
    for mio in [false, true] {
        let (mut pilot, observer) = Pilot::new(7);
        if mio {
            drive_mio(&mut pilot);
        } else {
            // Browser-like deferred callbacks advance exactly the same machine.
            pilot.step(Duration::ZERO);
            assert!(!pilot.completion.state.lock().unwrap().finished);
            pilot.step(Duration::ZERO);
        }
        assert!((observer.wait().unwrap() - 25.0).abs() < f32::EPSILON);
        assert_eq!(pilot.sequence, Some(1));
        assert_eq!(pilot.io.board.lock().unwrap().trace().len(), 1);
    }
}

#[test]
fn managed_mio_runner_allows_a_blocking_observer_on_another_thread() {
    let (mut pilot, observer) = Pilot::new(7);
    let driver = thread::spawn(move || drive_mio(&mut pilot));
    assert!((observer.wait().unwrap() - 25.0).abs() < f32::EPSILON);
    driver.join().unwrap();
}

#[test]
fn future_poll_only_observes_and_completion_wakes_the_registered_observer() {
    let (mut pilot, mut observer) = Pilot::new(7);
    let counter = Arc::new(CountWake::default());
    let waker = TaskWaker::from(Arc::clone(&counter));
    let mut cx = Context::from_waker(&waker);
    assert!(Pin::new(&mut observer).poll(&mut cx).is_pending());
    assert!(
        pilot.io.board.lock().unwrap().trace().is_empty(),
        "poll performs no I/O"
    );
    drive_mio(&mut pilot);
    assert_eq!(counter.0.load(Ordering::SeqCst), 1);
    let TaskPoll::Ready(result) = Pin::new(&mut observer).poll(&mut cx) else {
        panic!("the registered observer lost its completion");
    };
    assert!((result.unwrap() - 25.0).abs() < f32::EPSILON);
}

#[test]
fn completion_before_registration_is_visible_and_observer_drop_does_not_abort() {
    let (mut pilot, observer) = Pilot::new(7);
    drop(observer);
    pilot.step(Duration::ZERO);
    assert!(
        pilot.action.is_some(),
        "dropping observation leaves the issued operation alive"
    );
    pilot.step(Duration::ZERO);
    let mut replacement = Observer(Arc::clone(&pilot.completion));
    let counter = Arc::new(CountWake::default());
    let waker = TaskWaker::from(Arc::clone(&counter));
    let mut cx = Context::from_waker(&waker);
    let TaskPoll::Ready(result) = Pin::new(&mut replacement).poll(&mut cx) else {
        panic!("a completed operation needs no future wake");
    };
    assert!((result.unwrap() - 25.0).abs() < f32::EPSILON);
    assert_eq!(counter.0.load(Ordering::SeqCst), 0);
}

#[test]
fn driver_drop_wakes_future_and_blocking_observers_without_reactor_progress() {
    let (pilot, mut observer) = Pilot::new(7);
    let counter = Arc::new(CountWake::default());
    let waker = TaskWaker::from(Arc::clone(&counter));
    let mut cx = Context::from_waker(&waker);
    assert!(Pin::new(&mut observer).poll(&mut cx).is_pending());
    drop(pilot);
    assert_eq!(counter.0.load(Ordering::SeqCst), 1);
    assert!(matches!(
        Pin::new(&mut observer).poll(&mut cx),
        TaskPoll::Ready(Err(Error::Bus {
            source: BusError::Disconnected,
            ..
        }))
    ));

    let (pilot, observer) = Pilot::new(8);
    let waiting = thread::spawn(move || observer.wait());
    drop(pilot);
    assert!(matches!(
        waiting.join().unwrap(),
        Err(Error::Bus {
            source: BusError::Disconnected,
            ..
        })
    ));
}

#[test]
fn rx_poll_timeout_preserves_eight_leases_while_control_makes_progress() {
    let (mut pilot, observer) = Pilot::new(7);
    let script = pilot.open_rx();
    assert!(matches!(
        pilot
            .io
            .rx_recv(Duration::ZERO, &mut |_, _| panic!("no synthetic frame")),
        Err(BusError::Timeout)
    ));
    assert_eq!(script.lock().unwrap().pending, 8);
    drive_mio(&mut pilot);
    assert!((observer.wait().unwrap() - 25.0).abs() < f32::EPSILON);
    let state = script.lock().unwrap();
    assert_eq!(state.pending, 8);
    assert_eq!(
        state.submitted, 8,
        "RX timeout causes no replacement submission"
    );
    assert_eq!(state.cancellations, 0);
}

#[test]
fn a_cross_thread_sample_borrow_is_not_requeued_while_control_completes() {
    let (mut pilot, observer) = Pilot::new(7);
    let script = pilot.open_rx();
    script
        .lock()
        .unwrap()
        .waits
        .push_back(Ok(Some(sample_frame())));
    let mut rx: Rx<BoardQueue> = pilot.io.rx.take().unwrap();
    rx.receive(Duration::ZERO, &mut |payload, oob| {
        assert_eq!(oob, [3, 0]);
        assert_eq!(script.lock().unwrap().pending, 7);
        thread::scope(|scope| {
            let (release, waiting) = mpsc::sync_channel(0);
            let borrower = scope.spawn(move || {
                waiting.recv().unwrap();
                assert_eq!(payload, [0x5a; 8]);
            });
            drive_mio(&mut pilot);
            assert_eq!(
                script.lock().unwrap().pending,
                7,
                "borrowed buffer is not transport-owned"
            );
            assert!(pilot.completion.state.lock().unwrap().finished);
            release.send(()).unwrap();
            borrower.join().unwrap();
        });
    })
    .unwrap();
    pilot.io.rx = Some(rx);
    assert!((observer.wait().unwrap() - 25.0).abs() < f32::EPSILON);
    assert_eq!(
        script.lock().unwrap().pending,
        8,
        "requeue follows borrow release"
    );
    assert_eq!(script.lock().unwrap().submitted, 9);
}

#[test]
fn saturated_rx_does_not_block_abort_and_failed_cleanup_keeps_ownership() {
    let (mut pilot, observer) = Pilot::new(7);
    let script = pilot.open_rx();
    script.lock().unwrap().fail_cancel = true;
    pilot.step(Duration::ZERO);
    pilot.abort();
    assert!(matches!(
        observer.wait(),
        Err(Error::Bus {
            source: BusError::Unsupported(_),
            ..
        })
    ));
    assert!(matches!(pilot.io.rx_close(), Err(BusError::Timeout)));
    assert!(
        pilot.io.rx.is_some(),
        "cleanup failure cannot release the endpoint"
    );
    assert_eq!(script.lock().unwrap().pending, 8);
    assert!(pilot.io.rx_open(8).is_err());
    // The real pending IRQ cannot replace the cancellation result.
    let packet = pilot.io.read(Reply::Notification, Duration::ZERO).unwrap();
    pilot.on_event(
        Event::Notification {
            id: 7,
            packet: &packet,
        },
        Duration::ZERO,
    );
    assert!(pilot.completion.state.lock().unwrap().finished);
    assert!(pilot.completion.state.lock().unwrap().result.is_none());
    script.lock().unwrap().fail_cancel = false;
    pilot.io.rx_close().unwrap();
    assert_eq!(script.lock().unwrap().pending, 0);
}

#[test]
fn deadline_result_cannot_be_overwritten_by_a_late_irq() {
    let (mut pilot, mut observer) = Pilot::new(7);
    let counter = Arc::new(CountWake::default());
    let waker = TaskWaker::from(Arc::clone(&counter));
    let mut cx = Context::from_waker(&waker);
    assert!(Pin::new(&mut observer).poll(&mut cx).is_pending());
    pilot.step(Duration::ZERO);
    // The fake IRQ exists, but is deliberately not delivered to the operation.
    assert_eq!(pilot.io.notifications.len(), 1);
    pilot.advance(Duration::from_secs(1));
    assert_eq!(
        pilot.io.notifications.len(),
        1,
        "timer progression performs no I/O"
    );
    assert_eq!(counter.0.load(Ordering::SeqCst), 1);
    assert!(matches!(
        Pin::new(&mut observer).poll(&mut cx),
        TaskPoll::Ready(Err(Error::Bus {
            source: BusError::Timeout,
            ..
        }))
    ));
    let packet = [6, 0, 0, 0, 0x0c, 0x80, 0, 0];
    pilot.on_event(
        Event::Notification {
            id: 7,
            packet: &packet,
        },
        Duration::from_secs(2),
    );
    assert!(pilot.completion.state.lock().unwrap().finished);
    assert!(pilot.completion.state.lock().unwrap().result.is_none());
    assert_eq!(pilot.sequence, None);
}
