//! Real Linux FD registration plus a deterministic whole-transfer USB backend.
//! No usbfs node, device reset, kernel install, or hardware claim is performed.

use std::collections::VecDeque;
use std::io;
use std::num::NonZeroUsize;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, ThreadId};
use std::time::{Duration, Instant};

use mio::event::Source;
use mio::unix::SourceFd;
use mio::{Events, Interest, Poll, Registry, Token, Waker};
use rsl_usdr_sim::{BoardRevision, Op, SimBoard};

use super::{BoardUsb, simulated_device};
use crate::chips::tmp114::{TemperatureRead, Tmp114};
use crate::error::{Access, Error};
use crate::lowlevel::BusError;
use crate::reference::ReferenceClock;
use crate::transport::usb::bus::{Reply as Endpoint, UsbBus, UsbIo};
use crate::transport::usb::i2c::I2cTransaction;
use crate::transport::usb::machine::{Machine, Output, Step, Submission, Transfer};
use crate::transport::usb::native::{
    Adapter, Backend, Completion, NativeEvent, NativeUsb, Queues, Reply, Report, Request, Session,
    Turn, Work, Worker,
};
use crate::transport::usb::protocol::I2cCache;

/// Caller-owned tokens; the completion Waker is shared with other reactor producers.
const DEVICE: Token = Token(41);
/// One Waker per Poll, not one per native USB adapter.
const WAKE: Token = Token(42);
/// Hang protection only; ordering is established by channels and native readiness.
const GUARD: Duration = Duration::from_secs(5);

/// A deterministic backend barrier; no sleeps or elapsed-time races in test sequencing.
struct Pause {
    /// Reports entry before blocking the worker.
    entered: mpsc::SyncSender<()>,
    /// Holds only the fake kernel boundary, never a reactor lock.
    release: mpsc::Receiver<()>,
}

/// Reactor-side control of one intentionally stuck backend call.
struct Gate {
    /// Waits for the worker's explicit admission.
    entered: mpsc::Receiver<()>,
    /// Releases its syscall or cleanup.
    release: mpsc::SyncSender<()>,
}

impl Pause {
    /// Creates both barrier endpoints during setup.
    fn new() -> (Self, Gate) {
        let (entered, observed) = mpsc::sync_channel(1);
        let (release, resumed) = mpsc::sync_channel(1);
        (
            Self {
                entered,
                release: resumed,
            },
            Gate {
                entered: observed,
                release,
            },
        )
    }

    /// Signals before parking inside the fake backend.
    fn wait(self) {
        self.entered.send(()).unwrap();
        self.release.recv_timeout(GUARD).unwrap();
    }
}

impl Gate {
    /// Establishes a deterministic interleaving, bounded only to catch a broken test.
    fn entered(&self) {
        self.entered.recv_timeout(GUARD).unwrap();
    }
    /// Unblocks the owning worker after assertions on reactor progress/ownership.
    fn release(self) {
        self.release.send(()).unwrap();
    }
}

/// Boundary observations prove syscall/Drop thread affinity, not internal adapter fields.
#[derive(Default)]
struct Record {
    /// Work labels and their actual executing threads.
    calls: Vec<(&'static str, ThreadId)>,
}

/// Whole-transfer stand-in for the native queue owner; genuine production machines run above.
struct Fake {
    /// Real OS descriptor, registered as the fake's readiness; not a fake Waker source.
    fd: UnixStream,
    /// Retains its peer so readiness is writable rather than peer-closed.
    _peer: UnixStream,
    /// Existing independently modeled board at the whole-transfer boundary.
    io: BoardUsb,
    /// One completion, never an unbounded synthetic transfer queue.
    completion: Option<Completion>,
    /// Optional empty/exhausted turns before taking that completion.
    turns: VecDeque<bool>,
    /// Blocks exactly the next submit.
    submit_pause: Option<Pause>,
    /// Blocks exactly the next reap turn.
    turn_pause: Option<Pause>,
    /// Blocks cleanup after registration retirement.
    cleanup_pause: Option<Pause>,
    /// Injects a worker panic while retaining backend ownership outside unwind.
    panic_submit: bool,
    /// Injects an original OS failure at the submission boundary.
    submit_error: Option<BusError>,
    /// Native error from cleanup, observed separately from operation completion.
    cleanup_error: Option<BusError>,
    /// Shared boundary log; no adapter state is inspected.
    record: Arc<Mutex<Record>>,
    /// Signals backend Drop without joining the worker.
    dropped: mpsc::SyncSender<ThreadId>,
}

impl Fake {
    /// Builds a fresh board and a local kernel readiness source, with no hardware effects.
    fn new() -> (Self, Arc<Mutex<Record>>, mpsc::Receiver<ThreadId>) {
        let (fd, peer) = UnixStream::pair().unwrap();
        let record = Arc::new(Mutex::new(Record::default()));
        let (dropped, observed) = mpsc::sync_channel(1);
        let io = BoardUsb {
            script: Arc::default(),
            rx: None,
            tx: None,
            board: Arc::new(Mutex::new(SimBoard::new(BoardRevision::Rev3))),
            registers: VecDeque::new(),
            notifications: VecDeque::new(),
            sequence: 0,
            lut: 0,
            index: 0,
        };
        (
            Self {
                fd,
                _peer: peer,
                io,
                completion: None,
                turns: VecDeque::new(),
                submit_pause: None,
                turn_pause: None,
                cleanup_pause: None,
                panic_submit: false,
                submit_error: None,
                cleanup_error: None,
                record: Arc::clone(&record),
                dropped,
            },
            record,
            observed,
        )
    }

    /// Records only calls at the deliberate worker/backend boundary.
    fn record(&self, call: &'static str) {
        self.record
            .lock()
            .unwrap()
            .calls
            .push((call, thread::current().id()));
    }
}

/// The fake's socket as a readiness source, borrowed while the worker owns the fake; it
/// records the thread that drops it.
struct FakeSource(RawFd, Arc<Mutex<Record>>);

impl Drop for FakeSource {
    fn drop(&mut self) {
        self.1
            .lock()
            .unwrap()
            .calls
            .push(("source drop", thread::current().id()));
    }
}

impl Source for FakeSource {
    fn register(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        SourceFd(&self.0).register(registry, token, interests)
    }

    fn reregister(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        SourceFd(&self.0).reregister(registry, token, interests)
    }

    fn deregister(&mut self, registry: &Registry) -> io::Result<()> {
        SourceFd(&self.0).deregister(registry)
    }
}

impl Backend for Fake {
    fn readiness(&self) -> Result<Box<dyn Source + Send>, BusError> {
        Ok(Box::new(FakeSource(
            self.fd.as_raw_fd(),
            Arc::clone(&self.record),
        )))
    }

    fn submit(&mut self, request: Request) -> Result<(), BusError> {
        self.record("submit");
        if let Some(pause) = self.submit_pause.take() {
            pause.wait();
        }
        assert!(!self.panic_submit, "injected worker submission panic");
        if let Some(error) = self.submit_error.take() {
            return Err(error);
        }
        if Instant::now() >= request.expires {
            return Err(BusError::Timeout);
        }
        assert!(self.completion.is_none(), "one admitted native request");
        let result = match request.transfer {
            Transfer::Write(frame) => self
                .io
                .write(frame.as_bytes(), Duration::ZERO)
                .map(|()| Reply::Written),
            Transfer::Readback => self
                .io
                .read(Endpoint::Registers, Duration::ZERO)
                .map(Reply::Readback),
            Transfer::Notification => self
                .io
                .read(Endpoint::Notification, Duration::ZERO)
                .map(Reply::Notification),
        };
        self.completion = Some(Completion {
            id: request.id,
            result,
        });
        Ok(())
    }

    fn turn(&mut self, _: NonZeroUsize) -> Result<Turn, BusError> {
        self.record("reap");
        if let Some(pause) = self.turn_pause.take() {
            pause.wait();
        }
        if let Some(exhausted) = self.turns.pop_front() {
            return Ok(Turn {
                completion: None,
                exhausted,
            });
        }
        Ok(Turn {
            completion: self.completion.take(),
            exhausted: false,
        })
    }

    fn cleanup(&mut self) -> Result<(), BusError> {
        self.record("cleanup");
        if let Some(pause) = self.cleanup_pause.take() {
            pause.wait();
        }
        self.completion = None;
        self.cleanup_error.take().map_or(Ok(()), Err)
    }
}

/// Drop remains on the owning worker even when a reactor/observer disappears.
impl Drop for Fake {
    fn drop(&mut self) {
        self.record("drop");
        let _ = self.dropped.try_send(thread::current().id());
    }
}

/// Caller-owned Poll with one shared notifier; production attach never creates either.
fn reactor() -> (Poll, Arc<Waker>) {
    let poll = Poll::new().unwrap();
    let wake = Arc::new(Waker::new(poll.registry(), WAKE).unwrap());
    (poll, wake)
}

/// A bounded completion-driving turn.
fn budget() -> NonZeroUsize {
    NonZeroUsize::new(1).unwrap()
}

/// The real sensor/I²C machines provide the command, expected value and sequence transition.
fn operation(id: u64) -> (TemperatureRead, Machine, Submission) {
    let (temperature, request) = TemperatureRead::begin(Tmp114::usdr());
    let transaction = I2cTransaction::new(
        id,
        request.dev,
        &request.register,
        2,
        &mut I2cCache::default(),
        Duration::ZERO,
    )
    .unwrap();
    let mut machine = Machine::I2c(transaction);
    let Step::Submit(submission) = machine.start(Duration::ZERO).unwrap() else {
        panic!("a fresh machine submits its command");
    };
    (temperature, machine, submission)
}

/// Waits only in the caller's reactor, forwarding native device events as hints.
fn poll(adapter: &mut Adapter, reactor: &mut Poll) -> bool {
    let mut events = Events::with_capacity(8);
    reactor.poll(&mut events, Some(GUARD)).unwrap();
    assert!(
        !events.is_empty(),
        "native worker/readiness must notify its caller"
    );
    let notified = events.iter().any(|event| event.token() == WAKE);
    for event in &events {
        adapter.on_ready(event.token()).unwrap();
    }
    notified
}

/// Observes a worker result without joining, sleeping or driving any USB syscall.
fn completion(adapter: &mut Adapter, reactor: &mut Poll) -> Completion {
    let until = Instant::now() + GUARD;
    loop {
        if let Some(event) = adapter.try_event(Instant::now()).unwrap() {
            return event;
        }
        assert!(Instant::now() < until, "native completion deadline");
        poll(adapter, reactor);
    }
}

/// Cleanup is independently observable, even when the operation already failed.
fn cleanup(adapter: &mut Adapter, reactor: &mut Poll) -> Result<(), BusError> {
    let until = Instant::now() + GUARD;
    loop {
        if let Some(result) = adapter.try_cleanup() {
            return result;
        }
        assert!(Instant::now() < until, "native cleanup deadline");
        poll(adapter, reactor);
    }
}

#[test]
fn native_fd_and_worker_execute_the_production_temperature_transaction() {
    let (fake, record, dropped) = Fake::new();
    let (mut reactor, wake) = reactor();
    let other_producer = Arc::clone(&wake);
    let mut adapter = Adapter::attach(fake, reactor.registry(), DEVICE, wake, budget()).unwrap();
    let (mut temperature, mut machine, mut submission) = operation(9);
    let mut sequence = None;
    other_producer.wake().unwrap();
    loop {
        adapter.submit(&submission, Instant::now()).unwrap();
        match completion(&mut adapter, &mut reactor)
            .apply(&mut machine, Duration::ZERO, &mut sequence)
            .unwrap()
        {
            Step::Submit(next) => submission = next,
            Step::Done(Output::I2c { bytes, len }) => {
                let value = temperature.complete(Ok(&bytes[..len])).unwrap().unwrap();
                assert!((value - 25.0).abs() < f32::EPSILON);
                assert_eq!(sequence, Some(1));
                break;
            }
            Step::Done(other) => panic!("an I2C read ends with bytes: {other:?}"),
        }
    }
    adapter.close().unwrap();
    cleanup(&mut adapter, &mut reactor).unwrap();
    let owner = dropped.recv_timeout(GUARD).unwrap();
    assert_ne!(owner, thread::current().id());
    assert!(
        record
            .lock()
            .unwrap()
            .calls
            .iter()
            .all(|(_, executing)| *executing == owner),
        "all USB calls, the readiness's Drop and the backend's Drop run on one worker"
    );
    let calls = record.lock().unwrap().calls.clone();
    let position = |name| calls.iter().position(|(call, _)| *call == name).unwrap();
    assert!(
        position("source drop") < position("cleanup"),
        "the readiness is released before the device"
    );
    assert!(adapter.try_cleanup().is_none(), "cleanup is published once");
}

#[test]
fn exhausted_budget_reschedules_without_another_device_edge() {
    let (mut fake, record, _) = Fake::new();
    fake.turns.extend([true, true]);
    let (mut reactor, wake) = reactor();
    let mut adapter = Adapter::attach(fake, reactor.registry(), DEVICE, wake, budget()).unwrap();
    adapter.submit(&operation(1).2, Instant::now()).unwrap();
    assert!(matches!(
        completion(&mut adapter, &mut reactor).result,
        Ok(Reply::Written)
    ));
    assert_eq!(
        record
            .lock()
            .unwrap()
            .calls
            .iter()
            .filter(|(name, _)| *name == "reap")
            .count(),
        3
    );
    adapter.close().unwrap();
    cleanup(&mut adapter, &mut reactor).unwrap();
}

#[test]
fn readiness_during_a_worker_turn_survives_its_drained_report() {
    let (mut fake, record, _) = Fake::new();
    let (pause, gate) = Pause::new();
    fake.turn_pause = Some(pause);
    fake.turns.push_back(false);
    let (mut reactor, wake) = reactor();
    let mut adapter = Adapter::attach(fake, reactor.registry(), DEVICE, wake, budget()).unwrap();
    adapter.submit(&operation(1).2, Instant::now()).unwrap();
    // Consume the submit acknowledgement to schedule the first worker reap.
    while !poll(&mut adapter, &mut reactor) {}
    adapter.try_event(Instant::now()).unwrap();
    gate.entered();
    adapter.on_ready(DEVICE).unwrap();
    gate.release();
    assert!(matches!(
        completion(&mut adapter, &mut reactor).result,
        Ok(Reply::Written)
    ));
    assert_eq!(
        record
            .lock()
            .unwrap()
            .calls
            .iter()
            .filter(|(name, _)| *name == "reap")
            .count(),
        2
    );
    adapter.close().unwrap();
    cleanup(&mut adapter, &mut reactor).unwrap();
}

#[test]
fn a_stuck_submit_cannot_block_deadlines_or_release_worker_ownership() {
    let (mut fake, _, dropped) = Fake::new();
    let (pause, gate) = Pause::new();
    fake.submit_pause = Some(pause);
    let (mut reactor, wake) = reactor();
    let mut adapter = Adapter::attach(fake, reactor.registry(), DEVICE, wake, budget()).unwrap();
    adapter.submit(&operation(1).2, Instant::now()).unwrap();
    gate.entered();
    let deadline = adapter.deadline().unwrap();
    let event = adapter.try_event(deadline).unwrap().unwrap();
    assert!(matches!(event.result, Err(BusError::Timeout)));
    assert!(adapter.try_event(deadline).unwrap().is_none());
    assert!(adapter.try_cleanup().is_none());
    assert!(
        matches!(dropped.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "the worker owns stuck transfers after reactor timeout"
    );
    gate.release();
    cleanup(&mut adapter, &mut reactor).unwrap();
    assert_ne!(dropped.recv_timeout(GUARD).unwrap(), thread::current().id());
}

#[test]
fn dropping_the_adapter_does_not_wait_for_or_release_blocked_cleanup() {
    let (mut fake, _, dropped) = Fake::new();
    let (pause, gate) = Pause::new();
    fake.cleanup_pause = Some(pause);
    let fd = fake.fd.as_raw_fd();
    let (reactor, wake) = reactor();
    let mut adapter = Adapter::attach(fake, reactor.registry(), DEVICE, wake, budget()).unwrap();
    adapter.close().unwrap();
    gate.entered();
    assert!(adapter.try_cleanup().is_none());
    // Same FD can be registered again only after the old registration was removed.
    reactor
        .registry()
        .register(&mut SourceFd(&fd), Token(43), Interest::WRITABLE)
        .unwrap();
    reactor.registry().deregister(&mut SourceFd(&fd)).unwrap();
    drop(adapter);
    assert!(matches!(dropped.try_recv(), Err(mpsc::TryRecvError::Empty)));
    gate.release();
    assert_ne!(dropped.recv_timeout(GUARD).unwrap(), thread::current().id());
}

#[test]
fn a_lost_registration_still_retires_the_worker() {
    let (fake, _, dropped) = Fake::new();
    let fd = fake.fd.as_raw_fd();
    let (mut reactor, wake) = reactor();
    let mut adapter = Adapter::attach(fake, reactor.registry(), DEVICE, wake, budget()).unwrap();
    // The entry is gone, as when a caller deregisters the token itself; no retry can work.
    reactor.registry().deregister(&mut SourceFd(&fd)).unwrap();
    assert_eq!(
        adapter.close().unwrap_err().kind(),
        io::ErrorKind::NotFound,
        "the lost registration is reported"
    );
    cleanup(&mut adapter, &mut reactor).unwrap();
    assert_ne!(dropped.recv_timeout(GUARD).unwrap(), thread::current().id());
    assert!(
        adapter.close().is_ok(),
        "a retired adapter closes again quietly"
    );
}

#[test]
fn a_worker_panic_resolves_once_and_still_cleans_up_on_the_worker() {
    let (mut fake, _, dropped) = Fake::new();
    fake.panic_submit = true;
    let (mut reactor, wake) = reactor();
    let mut adapter = Adapter::attach(fake, reactor.registry(), DEVICE, wake, budget()).unwrap();
    adapter.submit(&operation(1).2, Instant::now()).unwrap();
    assert!(matches!(
        completion(&mut adapter, &mut reactor).result,
        Err(BusError::Disconnected)
    ));
    assert!(adapter.try_event(Instant::now()).unwrap().is_none());
    assert!(matches!(
        adapter.submit(&operation(2).2, Instant::now()),
        Err(BusError::Disconnected)
    ));
    cleanup(&mut adapter, &mut reactor).unwrap();
    assert_ne!(dropped.recv_timeout(GUARD).unwrap(), thread::current().id());
}

#[test]
fn original_terminal_error_survives_a_lost_registration() {
    let (mut fake, _, dropped) = Fake::new();
    fake.submit_error = Some(BusError::Io(io::Error::from_raw_os_error(5)));
    let fd = fake.fd.as_raw_fd();
    let (mut reactor, wake) = reactor();
    let mut adapter = Adapter::attach(fake, reactor.registry(), DEVICE, wake, budget()).unwrap();
    adapter.submit(&operation(1).2, Instant::now()).unwrap();
    reactor.registry().deregister(&mut SourceFd(&fd)).unwrap();
    assert!(
        matches!(completion(&mut adapter, &mut reactor).result, Err(BusError::Io(error)) if error.raw_os_error() == Some(5))
    );
    assert_eq!(
        adapter.take_retirement_error().unwrap().kind(),
        io::ErrorKind::NotFound
    );
    assert!(adapter.take_retirement_error().is_none());
    assert!(adapter.try_event(Instant::now()).unwrap().is_none());
    assert!(matches!(
        adapter.submit(&operation(2).2, Instant::now()),
        Err(BusError::Disconnected)
    ));
    cleanup(&mut adapter, &mut reactor).unwrap();
    assert_ne!(
        dropped.recv_timeout(GUARD).unwrap(),
        thread::current().id(),
        "a lost registration still retires the worker"
    );
}

#[test]
fn notification_transfers_are_the_fewest_whole_packets_holding_a_record() {
    assert!(matches!(
        Queues::notification_capacity(0),
        Err(BusError::Unsupported(_))
    ));
    // IN_NTFY_SIZE: one notification transfer carries at most 64 bytes.
    for packet in [8, 16, 64, 128, 512, 1024] {
        let capacity = Queues::notification_capacity(packet).unwrap();
        assert!(capacity >= 64, "packet size {packet} holds a whole record");
        assert_eq!(
            capacity % packet,
            0,
            "packet size {packet} ends on a packet"
        );
        assert!(
            capacity < 64 + packet,
            "packet size {packet} adds no spare packet"
        );
    }
}

#[test]
fn original_submission_and_cleanup_errors_are_observable_separately() {
    let (mut fake, _, _) = Fake::new();
    fake.submit_error = Some(BusError::Io(io::Error::from_raw_os_error(5)));
    fake.cleanup_error = Some(BusError::Io(io::Error::from_raw_os_error(16)));
    let (mut reactor, wake) = reactor();
    let mut adapter = Adapter::attach(fake, reactor.registry(), DEVICE, wake, budget()).unwrap();
    adapter.submit(&operation(1).2, Instant::now()).unwrap();
    assert!(
        matches!(completion(&mut adapter, &mut reactor).result, Err(BusError::Io(error)) if error.raw_os_error() == Some(5))
    );
    assert!(
        matches!(cleanup(&mut adapter, &mut reactor), Err(BusError::Io(error)) if error.raw_os_error() == Some(16))
    );
}

#[test]
fn an_expired_queued_request_never_reaches_backend_submission() {
    let (fake, record, dropped) = Fake::new();
    let source = fake.fd.as_raw_fd();
    let (mut reactor, wake) = reactor();
    let worker = Worker::spawn(fake, wake, budget()).unwrap();
    worker
        .send(Work::Submit(Request {
            id: 1,
            expires: Instant::now(),
            transfer: Transfer::Notification,
        }))
        .unwrap();
    let mut events = Events::with_capacity(8);
    loop {
        if let Some(report) = worker.report().unwrap() {
            assert!(matches!(
                report,
                Report::Failed {
                    id: 1,
                    error: BusError::Timeout
                }
            ));
            break;
        }
        reactor.poll(&mut events, Some(GUARD)).unwrap();
        assert!(!events.is_empty());
    }
    assert!(
        record.lock().unwrap().calls.is_empty(),
        "expiry precedes every backend call"
    );
    worker.retire(Box::new(FakeSource(source, Arc::clone(&record))));
    dropped.recv_timeout(GUARD).unwrap();
}

#[test]
fn overlapping_actions_and_reused_operation_ids_are_rejected_before_io() {
    let (fake, record, _) = Fake::new();
    let (mut reactor, wake) = reactor();
    let mut adapter = Adapter::attach(fake, reactor.registry(), DEVICE, wake, budget()).unwrap();
    adapter.submit(&operation(3).2, Instant::now()).unwrap();
    assert!(matches!(
        adapter.submit(&operation(4).2, Instant::now()),
        Err(BusError::Unsupported(_))
    ));
    completion(&mut adapter, &mut reactor);
    assert!(matches!(
        adapter.submit(&operation(3).2, Instant::now()),
        Err(BusError::Unsupported(_))
    ));
    assert_eq!(
        record
            .lock()
            .unwrap()
            .calls
            .iter()
            .filter(|(name, _)| *name == "submit")
            .count(),
        1
    );
    adapter.close().unwrap();
    // A stale edge delivered after close is ignored.
    adapter.on_ready(DEVICE).unwrap();
    cleanup(&mut adapter, &mut reactor).unwrap();
}

/// A fresh blocking-bus session for a revision 3 board: no sequence baseline yet.
fn session() -> Session {
    Session {
        i2c: I2cCache::default(),
        next_sequence: None,
        next_id: 0,
        rev3: true,
    }
}

/// Drives one native read through the caller's reactor until the handle reports it.
fn reading(native: &mut NativeUsb, reactor: &mut Poll) -> Result<f32, Error> {
    let until = Instant::now() + GUARD;
    let mut events = Events::with_capacity(8);
    loop {
        if let Some(NativeEvent::Temperature(result)) = native.poll(Instant::now()) {
            return result;
        }
        assert!(Instant::now() < until, "native temperature deadline");
        reactor.poll(&mut events, Some(GUARD)).unwrap();
        for event in &events {
            native.on_ready(event.token());
        }
    }
}

/// Waits in the reactor for the handle's cleanup report.
fn native_cleanup(native: &mut NativeUsb, reactor: &mut Poll) -> Result<(), BusError> {
    let until = Instant::now() + GUARD;
    let mut events = Events::with_capacity(8);
    loop {
        if let Some(result) = native.try_cleanup() {
            return result;
        }
        assert!(Instant::now() < until, "native cleanup deadline");
        reactor.poll(&mut events, Some(GUARD)).unwrap();
    }
}

#[test]
fn the_native_handle_reads_the_sensor_repeatedly_and_cleans_up_on_its_worker() {
    let (fake, _, dropped) = Fake::new();
    let (mut reactor, wake) = reactor();
    let mut native = NativeUsb::attach(fake, session(), reactor.registry(), DEVICE, wake).unwrap();
    // The second read is checked against the sequence number the first one established.
    for _ in 0..2 {
        native.start_temperature(Instant::now()).unwrap();
        let celsius = reading(&mut native, &mut reactor).unwrap();
        assert!(
            (celsius - 25.0).abs() < f32::EPSILON,
            "the sim reports 25 °C"
        );
    }
    native.close().unwrap();
    native_cleanup(&mut native, &mut reactor).unwrap();
    assert_ne!(dropped.recv_timeout(GUARD).unwrap(), thread::current().id());
}

#[test]
fn a_second_read_is_refused_while_one_is_pending() {
    let (fake, record, _) = Fake::new();
    let (mut reactor, wake) = reactor();
    let mut native = NativeUsb::attach(fake, session(), reactor.registry(), DEVICE, wake).unwrap();
    native.start_temperature(Instant::now()).unwrap();
    assert!(matches!(
        native.start_temperature(Instant::now()),
        Err(Error::Busy)
    ));
    assert!(
        reading(&mut native, &mut reactor).is_ok(),
        "the first read is unaffected"
    );
    let submits = record
        .lock()
        .unwrap()
        .calls
        .iter()
        .filter(|(name, _)| *name == "submit")
        .count();
    assert_eq!(
        submits, 2,
        "one command and one notification read; nothing for the refusal"
    );
}

#[test]
fn the_handle_continues_the_blocking_bus_notification_sequence() {
    let (fake, _, dropped) = Fake::new();
    let (mut reactor, wake) = reactor();
    // The blocking bus expects record 7 next; the fresh sim sends its first record.
    let carried = Session {
        next_sequence: Some(7),
        ..session()
    };
    let mut native = NativeUsb::attach(fake, carried, reactor.registry(), DEVICE, wake).unwrap();
    native.start_temperature(Instant::now()).unwrap();
    assert!(
        reading(&mut native, &mut reactor).is_err(),
        "a sequence gap at the hand-over fails the read"
    );
    assert!(
        native.start_temperature(Instant::now()).is_err(),
        "a failure closes the handle"
    );
    native_cleanup(&mut native, &mut reactor).unwrap();
    assert_ne!(dropped.recv_timeout(GUARD).unwrap(), thread::current().id());
}

#[test]
fn handing_over_the_blocking_bus_carries_its_notification_session() {
    let mut board = SimBoard::new(BoardRevision::Rev3);
    // HWID bit 30: USB, not PCIe, owns interrupt routing.
    let hwid = board.read_reg(19);
    board.write_reg(19, hwid | 0x4000_0000);
    let io = BoardUsb {
        script: Arc::default(),
        rx: None,
        tx: None,
        board: Arc::new(Mutex::new(board)),
        registers: VecDeque::new(),
        notifications: VecDeque::new(),
        sequence: 0,
        lut: 0,
        index: 0,
    };
    // No router setup: the hand-over's own operations are the bus's only ones.
    let bus = UsbBus::from_io(Box::new(io));
    // The hand-over reads the build and HWID registers, then checks the sensor with one I²C
    // read whose notification record sets the baseline.
    let (carried, _) = bus.hand_over().unwrap();
    assert_eq!(
        carried.next_id, 3,
        "register and I2C operations draw from one identity sequence"
    );
    assert!(
        carried.next_sequence.is_some(),
        "the sensor check's notification set the sequence baseline"
    );
}

/// Drives the handle's pending operation through the caller's reactor and its deadlines.
fn finish(native: &mut NativeUsb, reactor: &mut Poll) -> NativeEvent {
    let until = Instant::now() + GUARD;
    let mut events = Events::with_capacity(8);
    loop {
        if let Some(event) = native.poll(Instant::now()) {
            return event;
        }
        assert!(Instant::now() < until, "native operation deadline");
        let timeout = native.deadline().map_or(GUARD, |deadline| {
            deadline.saturating_duration_since(Instant::now())
        });
        reactor.poll(&mut events, Some(timeout)).unwrap();
        for event in &events {
            native.on_ready(event.token());
        }
    }
}

/// The board's operations from `start` on, without waits: native waits are caller
/// deadlines, which the board never sees.
fn operations(board: &Mutex<SimBoard>, start: usize) -> Vec<Op> {
    board.lock().unwrap().trace()[start..]
        .iter()
        .filter(|op| !matches!(op, Op::Sleep { .. }))
        .cloned()
        .collect()
}

/// A powered-up board's fake backend and a native handle over it, plus the device that
/// powered it (kept, so its drop does not power the board down mid-test).
fn powered(
    revision: BoardRevision,
    reactor: &Poll,
    wake: Arc<Waker>,
) -> (
    NativeUsb,
    Arc<Mutex<SimBoard>>,
    Arc<Mutex<Record>>,
    crate::Device,
) {
    let (device, board, _) = simulated_device(revision);
    let (mut fake, record, _) = Fake::new();
    fake.io.board = Arc::clone(&board);
    let carried = Session {
        rev3: revision == BoardRevision::Rev3,
        ..session()
    };
    let native = NativeUsb::attach(fake, carried, reactor.registry(), DEVICE, wake).unwrap();
    (native, board, record, device)
}

/// Submissions the fake backend has seen.
fn submits(record: &Mutex<Record>) -> usize {
    record
        .lock()
        .unwrap()
        .calls
        .iter()
        .filter(|(name, _)| *name == "submit")
        .count()
}

#[test]
fn the_native_identity_read_reports_what_the_gateware_registers_hold() {
    let (fake, _, dropped) = Fake::new();
    let board = Arc::clone(&fake.io.board);
    let (build, hwid) = {
        let mut board = board.lock().unwrap();
        board.write_reg(16, 0x5a5a_0001);
        (board.read_reg(16), board.read_reg(19))
    };
    let start = board.lock().unwrap().trace().len();
    let (mut reactor, wake) = reactor();
    let mut native = NativeUsb::attach(fake, session(), reactor.registry(), DEVICE, wake).unwrap();
    native.start_identity(Instant::now()).unwrap();
    let NativeEvent::Identity(Ok(identity)) = finish(&mut native, &mut reactor) else {
        panic!("the identity read succeeds");
    };
    assert_eq!(identity.gateware_build, build);
    assert_eq!(identity.hwid, hwid);
    assert_eq!(identity.revision, 3, "a revision 3 board");
    assert!(identity.has_rx && identity.has_tx);
    assert_eq!(
        operations(&board, start),
        [
            Op::RegRead {
                addr: 16,
                value: build
            },
            Op::RegRead {
                addr: 19,
                value: hwid
            },
        ],
        "the build word, then the hardware ID, as libusdr reads them"
    );
    native.close().unwrap();
    native_cleanup(&mut native, &mut reactor).unwrap();
    assert_ne!(dropped.recv_timeout(GUARD).unwrap(), thread::current().id());
}

#[test]
fn the_native_reference_switch_performs_the_blocking_devices_operations() {
    for (revision, reference) in [
        (BoardRevision::Rev3, ReferenceClock::Board),
        (BoardRevision::Rev1, ReferenceClock::Board),
        (
            BoardRevision::Rev3,
            ReferenceClock::External { hz: 40_000_000 },
        ),
        (BoardRevision::Rev2, ReferenceClock::Oscillator),
    ] {
        let (mut device, blocking, _) = simulated_device(revision);
        let start = blocking.lock().unwrap().trace().len();
        let outcome = device
            .set_reference_clock(reference)
            .map_err(|error| error.to_string());
        let expected = operations(&blocking, start);

        let (mut reactor, wake) = reactor();
        let (mut native, board, _, _device) = powered(revision, &reactor, wake);
        let start = board.lock().unwrap().trace().len();
        native
            .start_reference_clock(reference, Instant::now())
            .unwrap();
        let NativeEvent::ReferenceClock(result) = finish(&mut native, &mut reactor) else {
            panic!("the reference switch reports itself");
        };
        assert_eq!(
            result.map_err(|error| error.to_string()),
            outcome,
            "{revision:?} {reference:?}"
        );
        assert_eq!(
            operations(&board, start),
            expected,
            "{revision:?} {reference:?}: the same machine reaches the board"
        );
        native.close().unwrap();
        native_cleanup(&mut native, &mut reactor).unwrap();
    }
}

#[test]
fn a_sequence_wait_is_a_caller_deadline_with_no_transfer() {
    let (mut reactor, wake) = reactor();
    let (mut native, _, record, _device) = powered(BoardRevision::Rev3, &reactor, wake);
    let now = Instant::now();
    native
        .start_reference_clock(ReferenceClock::Board, now)
        .unwrap();
    // The switch begins with libusdr's 10 µs wait before its first state poll.
    let wait = native
        .deadline()
        .expect("a wait schedules the caller's timer");
    assert_eq!(wait, now + Duration::from_micros(10));
    assert!(native.poll(now).is_none(), "the wait has not passed yet");
    assert_eq!(native.deadline(), Some(wait), "no transfer was admitted");
    assert!(
        native.poll(wait).is_none(),
        "the state poll is now in flight"
    );
    let transfer = native.deadline().expect("the state poll has a deadline");
    assert!(
        transfer >= wait + Duration::from_secs(1),
        "the passed wait admitted the state poll's command, with its own timeout"
    );
    assert!(matches!(
        finish(&mut native, &mut reactor),
        NativeEvent::ReferenceClock(Ok(()))
    ));
    assert!(submits(&record) > 0);
}

#[test]
fn a_missing_input_clock_is_reported_and_leaves_the_handle_open() {
    let (mut fake, _, _) = Fake::new();
    // Revisions 1 and 2 carry no on-board oscillator.
    fake.io.board = Arc::new(Mutex::new(
        SimBoard::new(BoardRevision::Rev2).with_oscillator_off(),
    ));
    let (mut reactor, wake) = reactor();
    let carried = Session {
        rev3: false,
        ..session()
    };
    let mut native = NativeUsb::attach(fake, carried, reactor.registry(), DEVICE, wake).unwrap();
    native
        .start_reference_clock(ReferenceClock::Oscillator, Instant::now())
        .unwrap();
    assert!(matches!(
        finish(&mut native, &mut reactor),
        NativeEvent::ReferenceClock(Err(Error::ClockInputMissing))
    ));
    native.start_identity(Instant::now()).unwrap();
    assert!(
        matches!(
            finish(&mut native, &mut reactor),
            NativeEvent::Identity(Ok(_))
        ),
        "the transport is still in step after a well-formed failure report"
    );
}

#[test]
fn a_failed_register_transfer_names_its_register_and_closes_the_handle() {
    let (mut fake, _, dropped) = Fake::new();
    fake.submit_error = Some(BusError::Io(io::Error::from_raw_os_error(5)));
    let (mut reactor, wake) = reactor();
    let mut native = NativeUsb::attach(fake, session(), reactor.registry(), DEVICE, wake).unwrap();
    native.start_identity(Instant::now()).unwrap();
    assert!(matches!(
        finish(&mut native, &mut reactor),
        NativeEvent::Identity(Err(Error::Bus {
            chip: "FPGA",
            access: Access::Read,
            reg: 16,
            source: BusError::Io(ref error),
        })) if error.raw_os_error() == Some(5)
    ));
    assert!(
        native.start_identity(Instant::now()).is_err(),
        "a transfer failure closes the handle"
    );
    native_cleanup(&mut native, &mut reactor).unwrap();
    assert_ne!(dropped.recv_timeout(GUARD).unwrap(), thread::current().id());
}

#[test]
fn a_second_operation_or_a_bad_argument_is_refused_without_io_or_closing() {
    let (fake, record, _) = Fake::new();
    let (mut reactor, wake) = reactor();
    let mut native = NativeUsb::attach(fake, session(), reactor.registry(), DEVICE, wake).unwrap();
    assert!(matches!(
        native.start_reference_clock(ReferenceClock::External { hz: 0 }, Instant::now()),
        Err(Error::UnsupportedReferenceClock(0))
    ));
    native.start_identity(Instant::now()).unwrap();
    assert!(matches!(
        native.start_reference_clock(ReferenceClock::Board, Instant::now()),
        Err(Error::Busy)
    ));
    assert!(matches!(
        native.start_temperature(Instant::now()),
        Err(Error::Busy)
    ));
    assert!(matches!(
        native.start_identity(Instant::now()),
        Err(Error::Busy)
    ));
    assert!(matches!(
        finish(&mut native, &mut reactor),
        NativeEvent::Identity(Ok(_))
    ));
    assert_eq!(
        submits(&record),
        4,
        "two reads, each a command and a readback"
    );
}

#[test]
fn readback_transfers_need_a_high_speed_endpoint_and_hold_the_largest_reply() {
    for packet in [64, 256] {
        assert!(
            matches!(
                Queues::readback_capacity(packet),
                Err(BusError::Unsupported(_))
            ),
            "packet size {packet} could end a 256-byte readback without a short packet"
        );
    }
    assert_eq!(Queues::readback_capacity(512).unwrap(), 512);
    assert_eq!(Queues::readback_capacity(1024).unwrap(), 1024);
}

#[test]
fn handing_over_refuses_an_unsupported_revision_and_carries_revision_3() {
    for (revision, rev3) in [(3, true), (1, false)] {
        let mut board = SimBoard::new(BoardRevision::Rev3);
        let hwid = board.read_reg(19) & !0xff00;
        board.write_reg(19, hwid | 0x4000_0000 | revision << 8);
        let mut bus = UsbBus::from_io(Box::new(board_usb(board)));
        bus.initialize().unwrap();
        let (carried, _) = bus.hand_over().unwrap();
        assert_eq!(carried.rev3, rev3, "revision {revision}");
    }
    let mut board = SimBoard::new(BoardRevision::Rev3);
    let hwid = board.read_reg(19) & !0xff00;
    board.write_reg(19, hwid | 0x4000_0000 | 4 << 8);
    let mut bus = UsbBus::from_io(Box::new(board_usb(board)));
    bus.initialize().unwrap();
    assert!(matches!(
        bus.hand_over(),
        Err(Error::UnsupportedRevision(4))
    ));
}

/// The whole-transfer board model over `board`.
fn board_usb(board: SimBoard) -> BoardUsb {
    BoardUsb {
        script: Arc::default(),
        rx: None,
        tx: None,
        board: Arc::new(Mutex::new(board)),
        registers: VecDeque::new(),
        notifications: VecDeque::new(),
        sequence: 0,
        lut: 0,
        index: 0,
    }
}
