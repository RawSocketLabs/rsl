//! Real Linux FD registration plus a deterministic whole-transfer USB backend.
//! No usbfs node, device reset, kernel install, or hardware claim is performed.

use std::collections::VecDeque;
use std::future::Future;
use std::io;
use std::mem;
use std::num::NonZeroUsize;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::pin::pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::task::{self, Context, Wake, Waker as TaskWaker};
use std::thread::{self, ThreadId};
use std::time::{Duration, Instant};

use mio::event::Source;
use mio::unix::SourceFd;
use mio::{Events, Interest, Poll, Registry, Token, Waker};
use num_complex::Complex;
use rsl_usdr_sim::{BoardRevision, Op, PllLock, SimBoard};

use super::{BoardUsb, simulated_device};
use crate::Device;
use crate::board::{RxGain, RxPath, RxThrottle, TxGain, TxPath};
use crate::chips::tmp114::{TemperatureRead, Tmp114};
use crate::error::{Access, Error};
use crate::fpga::IqImbalance;
use crate::frontend::FrontEndSelection;
use crate::lowlevel::{Bus, BusError, TxFill};
use crate::portable::radio::Chains;
use crate::reference::ReferenceClock;
use crate::stream::{HostFormat, RxConfig, StreamsConfig, SyncMode, TxConfig, TxMute, WireFormat};
use crate::thermal::ThermalPolicy;
use crate::transport::usb::bus::{Reply as Endpoint, UsbBus, UsbIo};
use crate::transport::usb::i2c::I2cTransaction;
use crate::transport::usb::machine::{Machine, Output, Step, Submission, Transfer};
use crate::transport::usb::native::{
    Adapter, AsyncUsb, Backend, BoardState, Completion, Filing, ManagedUsb, NativeEvent, NativeUsb,
    Queues, RX_BACKLOG, Reply, Report, Request, Session, StreamCommand, StreamOutcome, Turn, Work,
    Worker,
};
use crate::transport::usb::protocol::I2cCache;
use crate::transport::usb::tx::TxStatus;

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
    /// The next reap turn panics on the worker.
    panic_turn: bool,
}

/// Whole-transfer stand-in for the native queue owner; genuine production machines run above.
/// `I` is the transfer boundary: a fresh board model, or the one a blocking bus handed over.
struct Fake<I = BoardUsb> {
    /// Real OS descriptor, registered as the fake's readiness; not a fake Waker source.
    fd: UnixStream,
    /// Retains its peer so readiness is writable rather than peer-closed.
    _peer: UnixStream,
    /// Existing independently modeled board at the whole-transfer boundary.
    io: I,
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
    /// Injects a failure at the next reap turn, such as the device hanging up.
    turn_error: Option<BusError>,
    /// Native error from cleanup, observed separately from operation completion.
    cleanup_error: Option<BusError>,
    /// When set, RX blocks the worker may take: as on usbfs, only a reap turn files them,
    /// a whole ring's worth each turn. Unset, every block is filed at once.
    rx_filed: Option<usize>,
    /// Shared boundary log; no adapter state is inspected.
    record: Arc<Mutex<Record>>,
    /// Signals backend Drop without joining the worker.
    dropped: mpsc::SyncSender<ThreadId>,
}

impl Fake {
    /// Builds a fresh board and a local kernel readiness source, with no hardware effects.
    fn new() -> (Self, Arc<Mutex<Record>>, mpsc::Receiver<ThreadId>) {
        Self::over(BoardUsb {
            script: Arc::default(),
            rx: None,
            tx: None,
            board: Arc::new(Mutex::new(SimBoard::new(BoardRevision::Rev3))),
            registers: VecDeque::new(),
            notifications: VecDeque::new(),
            sequence: 0,
            lut: 0,
            index: 0,
        })
    }
}

impl<I> Fake<I> {
    /// A local kernel readiness source over the transfer boundary `io`.
    fn over(io: I) -> (Self, Arc<Mutex<Record>>, mpsc::Receiver<ThreadId>) {
        let (fd, peer) = UnixStream::pair().unwrap();
        let record = Arc::new(Mutex::new(Record::default()));
        let (dropped, observed) = mpsc::sync_channel(1);
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
                turn_error: None,
                cleanup_error: None,
                rx_filed: None,
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

impl<I: UsbIo + 'static> Backend for Fake<I> {
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
        let panic = mem::take(&mut self.record.lock().unwrap().panic_turn);
        assert!(!panic, "injected worker turn panic");
        if let Some(error) = self.turn_error.take() {
            return Err(error);
        }
        if let Some(pause) = self.turn_pause.take() {
            pause.wait();
        }
        if let Some(filed) = &mut self.rx_filed {
            *filed = 8;
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
        let streams = self.io.rx_close().and(self.io.tx_close());
        self.cleanup_error.take().map_or(streams, Err)
    }

    fn rx_open(&mut self, block_bytes: u32) -> Result<(), BusError> {
        self.record("rx open");
        self.io.rx_open(block_bytes)
    }

    fn rx_receive(&mut self, consume: &mut dyn FnMut(&[u8], [u64; 2])) -> Result<Filing, BusError> {
        self.record("rx receive");
        match &mut self.rx_filed {
            Some(0) => return Ok(Filing::Empty),
            Some(filed) => *filed -= 1,
            None => {}
        }
        filing(self.io.rx_recv(Duration::ZERO, consume))
    }

    fn rx_close(&mut self) -> Result<(), BusError> {
        self.record("rx close");
        self.io.rx_close()
    }

    fn tx_open(&mut self, block_bytes: u32, sample_bytes: u32) -> Result<(), BusError> {
        self.record("tx open");
        self.io.tx_open(block_bytes, sample_bytes)
    }

    fn tx_acquire(&mut self) -> Result<Filing, BusError> {
        self.record("tx acquire");
        filing(self.io.tx_acquire(Duration::ZERO))
    }

    fn tx_commit(
        &mut self,
        time: Option<u64>,
        status: [u32; 4],
        fill: &mut TxFill<'_>,
    ) -> Result<(), BusError> {
        self.record("tx commit");
        self.io.tx_commit(time, Some(status), fill)
    }

    fn tx_close(&mut self) -> Result<(), BusError> {
        self.record("tx close");
        self.io.tx_close()
    }
}

/// A blocking-bus stream step at zero timeout, as the native backend reports it: the
/// board's queues never wait, so a timeout means nothing has completed.
fn filing(step: Result<(), BusError>) -> Result<Filing, BusError> {
    match step {
        Ok(()) => Ok(Filing::Taken),
        Err(BusError::Timeout) => Ok(Filing::Empty),
        Err(error) => Err(error),
    }
}

/// Drop remains on the owning worker even when a reactor/observer disappears.
impl<I> Drop for Fake<I> {
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
        adapter.on_ready(event.token());
    }
    notified
}

/// Observes a worker result without joining, sleeping or driving any USB syscall.
fn completion(adapter: &mut Adapter, reactor: &mut Poll) -> Completion {
    let until = Instant::now() + GUARD;
    loop {
        if let Some(event) = adapter.try_event(Instant::now()) {
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
    adapter.try_event(Instant::now());
    gate.entered();
    adapter.on_ready(DEVICE);
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
    let event = adapter.try_event(deadline).unwrap();
    assert!(matches!(event.result, Err(BusError::Timeout)));
    assert!(adapter.try_event(deadline).is_none());
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
    assert!(adapter.try_event(Instant::now()).is_none());
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
    assert!(adapter.try_event(Instant::now()).is_none());
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
    adapter.on_ready(DEVICE);
    cleanup(&mut adapter, &mut reactor).unwrap();
}

/// A fresh blocking-bus session for a revision 3 board: no sequence baseline yet.
fn session() -> Session {
    revision_session(true)
}

/// A fresh blocking-bus session for a board of revision 3 if `rev3`, else 1 or 2, with
/// a fresh board's IQ corrections.
fn revision_session(rev3: bool) -> Session {
    Session {
        i2c: I2cCache::default(),
        next_sequence: None,
        next_id: 0,
        board: BoardState {
            rev3,
            chains: Chains {
                rx: true,
                tx: true,
                extended_tx_core: false,
            },
            radio: None,
            rx_iq_imbalance: IqImbalance::default(),
            tx_iq_imbalance: IqImbalance::default(),
        },
        tx_status: TxStatus::default(),
        thermal: None,
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
    // The hand-over reads the build and HWID registers, holds the RF transceiver in reset,
    // then checks the sensor with one I²C read whose notification record sets the baseline.
    let carried = bus.hand_over().unwrap().session;
    assert_eq!(
        carried.next_id, 4,
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
) -> (NativeUsb, Arc<Mutex<SimBoard>>, Arc<Mutex<Record>>, Device) {
    let (device, board, _) = simulated_device(revision);
    let (mut fake, record, _) = Fake::new();
    fake.io.board = Arc::clone(&board);
    let carried = revision_session(revision == BoardRevision::Rev3);
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
    let carried = revision_session(false);
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
        let carried = bus.hand_over().unwrap().session;
        assert_eq!(carried.board.rev3, rev3, "revision {revision}");
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

/// The threads the fake backend recorded `name` calls on.
fn calls(record: &Mutex<Record>, name: &str) -> Vec<ThreadId> {
    record
        .lock()
        .unwrap()
        .calls
        .iter()
        .filter(|(call, _)| *call == name)
        .map(|(_, thread)| *thread)
        .collect()
}

/// The TX core's status-word reads among the board's operations.
fn status_reads(board: &Mutex<SimBoard>) -> usize {
    operations(board, 0)
        .iter()
        .filter(|op| matches!(op, Op::RegRead { addr: 28, .. }))
        .count()
}

/// One 1024-sample `ci16` block: four bytes a sample.
const RX_BLOCK: u32 = 4096;

/// A tuned board whose RX stream engine runs 1024-sample `ci16` packets, dropping
/// `losses` packets before successive blocks; started by the blocking driver.
fn rx_streaming(losses: &[u32]) -> (Device, Arc<Mutex<SimBoard>>) {
    let (mut device, board, script) = simulated_device(BoardRevision::Rev3);
    script.lock().unwrap().losses.extend(losses);
    device.set_rx_sample_rate(20_000_000).unwrap();
    device.set_rx_frequency(1_000_000_000).unwrap();
    device.start_rx_stream(1024).unwrap();
    (device, board)
}

/// Waits through the reactor for the handle's next event and checks it is `expected`.
fn expect_event(
    native: &mut NativeUsb,
    reactor: &mut Poll,
    expected: fn(&NativeEvent) -> bool,
    what: &str,
) {
    let event = finish(native, reactor);
    assert!(expected(&event), "{what}: got {event:?}");
}

#[test]
fn native_rx_delivers_the_samples_and_losses_the_blocking_receiver_does() {
    // Two boards run the same stream and drop the same packets: the blocking driver reads
    // one, the native handle the other, its worker copying each block off its transfer.
    let losses = [0, 2, 0, 1];
    let (mut blocking, _) = rx_streaming(&losses);
    let mut samples = vec![Complex::new(0, 0); 1024];
    let expected: Vec<_> = losses
        .iter()
        .map(|_| {
            let packet = blocking.receive(&mut samples, Duration::ZERO).unwrap();
            (samples.clone(), packet.lost_packets)
        })
        .collect();

    let (_device, board) = rx_streaming(&[]);
    let (mut fake, record, _) = Fake::new();
    fake.io.board = board;
    fake.io.script.lock().unwrap().losses.extend(losses);
    let script = Arc::clone(&fake.io.script);
    let (mut reactor, wake) = reactor();
    let mut native = NativeUsb::attach(fake, session(), reactor.registry(), DEVICE, wake).unwrap();
    native.start_rx_open(RX_BLOCK).unwrap();
    expect_event(
        &mut native,
        &mut reactor,
        |event| matches!(event, NativeEvent::RxOpened(Ok(()))),
        "RX opens",
    );
    let mut lost = 0;
    for (index, (samples, lost_packets)) in expected.into_iter().enumerate() {
        let NativeEvent::Received(Ok(block)) = finish(&mut native, &mut reactor) else {
            panic!("block {index} arrives");
        };
        let decoded: Vec<_> = block
            .payload
            .chunks_exact(4)
            .map(|sample| {
                Complex::new(
                    i16::from_le_bytes([sample[0], sample[1]]),
                    i16::from_le_bytes([sample[2], sample[3]]),
                )
            })
            .collect();
        assert!(decoded == samples, "block {index} carries the same samples");
        lost += block.oob[0] & 0xff_ffff;
        assert_eq!(lost, lost_packets, "block {index} reports the same losses");
    }
    // Every block taken was requeued; this fake files blocks as fast as they are asked for,
    // so the exact count depends on scheduling (the gated test below pins it).
    let taken = script.lock().unwrap().submitted - 8;
    assert!(
        taken <= 4 + RX_BACKLOG,
        "the worker copies at most the backlog ahead of the caller ({taken} taken)"
    );
    native.start_rx_close().unwrap();
    expect_event(
        &mut native,
        &mut reactor,
        |event| matches!(event, NativeEvent::RxClosed(Ok(()))),
        "RX closes",
    );
    let state = script.lock().unwrap();
    assert_eq!(state.cancellations, 1, "closing cancels the ring once");
    assert_eq!(state.pending, 0, "every transfer came back");
    drop(state);
    let test = thread::current().id();
    for call in ["rx open", "rx receive", "rx close"] {
        let threads = calls(&record, call);
        assert!(!threads.is_empty(), "{call} ran");
        assert!(
            threads.iter().all(|thread| *thread != test),
            "{call} runs on the worker, never the reactor"
        );
    }
}

#[test]
fn an_rx_failure_ends_only_the_stream_until_it_is_closed() {
    let (fake, _, _) = Fake::new();
    // A transfer that ends short of a whole block and its trailer.
    fake.io
        .script
        .lock()
        .unwrap()
        .waits
        .push_back(Ok(Some(vec![0; 10])));
    let (mut reactor, wake) = reactor();
    let mut native = NativeUsb::attach(fake, session(), reactor.registry(), DEVICE, wake).unwrap();
    native.start_rx_open(RX_BLOCK).unwrap();
    expect_event(
        &mut native,
        &mut reactor,
        |event| matches!(event, NativeEvent::RxOpened(Ok(()))),
        "RX opens",
    );
    expect_event(
        &mut native,
        &mut reactor,
        |event| {
            matches!(event, NativeEvent::Received(Err(Error::Stream(BusError::Io(error))))
                if error.kind() == io::ErrorKind::InvalidData)
        },
        "a malformed block fails the stream",
    );
    native.start_temperature(Instant::now()).unwrap();
    expect_event(
        &mut native,
        &mut reactor,
        |event| matches!(event, NativeEvent::Temperature(Ok(_))),
        "the control plane is unaffected",
    );
    assert!(matches!(
        native.start_rx_open(RX_BLOCK),
        Err(Error::AlreadyStreaming)
    ));
    native.start_rx_close().unwrap();
    expect_event(
        &mut native,
        &mut reactor,
        |event| matches!(event, NativeEvent::RxClosed(Ok(()))),
        "the failed stream closes",
    );
    native.start_rx_open(RX_BLOCK).unwrap();
    expect_event(
        &mut native,
        &mut reactor,
        |event| matches!(event, NativeEvent::RxOpened(Ok(()))),
        "a closed stream opens again",
    );
}

#[test]
fn a_failed_rx_cancellation_keeps_the_endpoint_until_a_retry_succeeds() {
    let (fake, _, _) = Fake::new();
    let script = Arc::clone(&fake.io.script);
    let (mut reactor, wake) = reactor();
    let mut native = NativeUsb::attach(fake, session(), reactor.registry(), DEVICE, wake).unwrap();
    native.start_rx_open(RX_BLOCK).unwrap();
    expect_event(
        &mut native,
        &mut reactor,
        |event| matches!(event, NativeEvent::RxOpened(Ok(()))),
        "RX opens",
    );
    script.lock().unwrap().fail_cancel = true;
    native.start_rx_close().unwrap();
    expect_event(
        &mut native,
        &mut reactor,
        |event| {
            matches!(
                event,
                NativeEvent::RxClosed(Err(Error::Stream(BusError::Timeout)))
            )
        },
        "the cancellation fails",
    );
    assert_eq!(
        script.lock().unwrap().pending,
        8,
        "the worker still owns every transfer"
    );
    assert!(matches!(
        native.start_rx_open(RX_BLOCK),
        Err(Error::AlreadyStreaming)
    ));
    script.lock().unwrap().fail_cancel = false;
    native.start_rx_close().unwrap();
    expect_event(
        &mut native,
        &mut reactor,
        |event| matches!(event, NativeEvent::RxClosed(Ok(()))),
        "a retried close succeeds",
    );
    assert_eq!(script.lock().unwrap().pending, 0);
}

/// 130 sends, across two status-read intervals: lengths and times vary, and a three-sample
/// block ends the run. Each is (time, bytes).
fn sends() -> Vec<(Option<u64>, Vec<u8>)> {
    (0..130_u32)
        .map(|index| {
            let time = (index % 3 != 0).then(|| u64::from(index) * 4096);
            let samples = if index == 129 { 3 } else { 1 + index % 1024 };
            let bytes = (0..samples * 4)
                .map(|byte| u8::try_from((byte + index) % 251).unwrap())
                .collect();
            (time, bytes)
        })
        .collect()
}

#[test]
fn native_tx_frames_and_reads_status_as_the_blocking_bus_does() {
    // The blocking bus over one fresh board...
    let reference = board_usb(SimBoard::new(BoardRevision::Rev3));
    let (blocking_board, blocking_script) =
        (Arc::clone(&reference.board), Arc::clone(&reference.script));
    blocking_script.lock().unwrap().tx_finished = usize::MAX;
    let mut bus = UsbBus::from_io(Box::new(reference));
    bus.tx_stream_open(4096, 4).unwrap();
    let mut expected = Vec::new();
    for (time, bytes) in sends() {
        let mut status = None;
        let _ = bus
            .tx_stream_send(Duration::ZERO, time, &mut |buffer, words| {
                status = words;
                buffer[..bytes.len()].copy_from_slice(&bytes);
                bytes.len()
            })
            .unwrap();
        expected.push(status.unwrap());
    }
    bus.tx_stream_close().unwrap();

    // ...and the native handle over another.
    let (fake, record, _) = Fake::new();
    let (board, script) = (Arc::clone(&fake.io.board), Arc::clone(&fake.io.script));
    script.lock().unwrap().tx_finished = usize::MAX;
    let (mut reactor, wake) = reactor();
    let mut native = NativeUsb::attach(fake, session(), reactor.registry(), DEVICE, wake).unwrap();
    native.start_tx_open(4096, 4).unwrap();
    expect_event(
        &mut native,
        &mut reactor,
        |event| matches!(event, NativeEvent::TxOpened(Ok(()))),
        "TX opens",
    );
    for (index, (time, bytes)) in sends().into_iter().enumerate() {
        native
            .start_send(time, &bytes, GUARD, Instant::now())
            .unwrap();
        let NativeEvent::Sent(Ok(words)) = finish(&mut native, &mut reactor) else {
            panic!("send {index} is queued");
        };
        assert_eq!(
            words, expected[index],
            "send {index} reports the same status"
        );
    }
    native.start_tx_close().unwrap();
    expect_event(
        &mut native,
        &mut reactor,
        |event| matches!(event, NativeEvent::TxClosed(Ok(()))),
        "TX closes",
    );

    assert!(
        script.lock().unwrap().tx_frames == blocking_script.lock().unwrap().tx_frames,
        "the same frames, headers included, in the same order"
    );
    assert_eq!(
        operations(&board, 0),
        operations(&blocking_board, 0),
        "the same status reads at the same sends"
    );
    assert_eq!(
        status_reads(&board),
        3,
        "libusdr's tx_stat_rate: the first send's, the 65th's and the 129th's"
    );
    assert_eq!(script.lock().unwrap().tx_cancellations, 1);
    let test = thread::current().id();
    for call in ["tx open", "tx acquire", "tx commit", "tx close"] {
        let threads = calls(&record, call);
        assert!(!threads.is_empty(), "{call} ran");
        assert!(
            threads.iter().all(|thread| *thread != test),
            "{call} runs on the worker"
        );
    }
}

#[test]
fn a_full_tx_ring_times_out_uncounted_until_a_transfer_finishes() {
    let (fake, _, _) = Fake::new();
    let (board, script) = (Arc::clone(&fake.io.board), Arc::clone(&fake.io.script));
    let (mut reactor, wake) = reactor();
    let mut native = NativeUsb::attach(fake, session(), reactor.registry(), DEVICE, wake).unwrap();
    native.start_tx_open(4096, 4).unwrap();
    expect_event(
        &mut native,
        &mut reactor,
        |event| matches!(event, NativeEvent::TxOpened(Ok(()))),
        "TX opens",
    );
    // libusdr's 32 transfers, none of which the device finishes.
    for _ in 0..32 {
        native
            .start_send(None, &[1; 64], GUARD, Instant::now())
            .unwrap();
        expect_event(
            &mut native,
            &mut reactor,
            |event| matches!(event, NativeEvent::Sent(Ok(_))),
            "a free transfer takes the send",
        );
    }
    let now = Instant::now();
    native
        .start_send(None, &[2; 64], Duration::from_millis(5), now)
        .unwrap();
    assert!(matches!(native.start_temperature(now), Err(Error::Busy)));
    expect_event(
        &mut native,
        &mut reactor,
        |event| matches!(event, NativeEvent::Sent(Err(Error::Timeout))),
        "a full ring times out",
    );
    assert_eq!(script.lock().unwrap().tx_frames.len(), 32);
    script.lock().unwrap().tx_finished = 1;
    native
        .start_send(None, &[3; 64], GUARD, Instant::now())
        .unwrap();
    // The device's completion edge.
    native.on_ready(DEVICE);
    expect_event(
        &mut native,
        &mut reactor,
        |event| matches!(event, NativeEvent::Sent(Ok(_))),
        "a finished transfer takes the next send",
    );
    let frames = script.lock().unwrap().tx_frames.clone();
    assert_eq!(frames.len(), 33);
    assert_eq!(frames[32][16..], [3; 64], "the later send's own block");
    assert_eq!(
        status_reads(&board),
        1,
        "only the first send read the status: the timed-out send was not counted"
    );
}

#[test]
fn closing_the_handle_cancels_its_open_streams_on_the_worker() {
    let (fake, record, dropped) = Fake::new();
    let script = Arc::clone(&fake.io.script);
    let (mut reactor, wake) = reactor();
    let mut native = NativeUsb::attach(fake, session(), reactor.registry(), DEVICE, wake).unwrap();
    native.start_rx_open(RX_BLOCK).unwrap();
    expect_event(
        &mut native,
        &mut reactor,
        |event| matches!(event, NativeEvent::RxOpened(Ok(()))),
        "RX opens",
    );
    native.start_tx_open(4096, 4).unwrap();
    expect_event(
        &mut native,
        &mut reactor,
        |event| matches!(event, NativeEvent::TxOpened(Ok(()))),
        "TX opens",
    );
    native
        .start_send(Some(7), &[9; 128], GUARD, Instant::now())
        .unwrap();
    expect_event(
        &mut native,
        &mut reactor,
        |event| matches!(event, NativeEvent::Sent(Ok(_))),
        "the send is queued",
    );
    native.close().unwrap();
    assert!(
        native.poll(Instant::now()).is_none(),
        "a closed handle reports nothing more"
    );
    native_cleanup(&mut native, &mut reactor).unwrap();
    let worker = dropped.recv_timeout(GUARD).unwrap();
    assert_ne!(worker, thread::current().id());
    let state = script.lock().unwrap();
    assert_eq!(state.cancellations, 1, "RX was cancelled");
    assert_eq!(state.pending, 0);
    assert_eq!(state.tx_cancellations, 1, "TX was cancelled");
    drop(state);
    assert!(
        calls(&record, "cleanup")
            .iter()
            .all(|thread| *thread == worker),
        "cancellation ran on the worker"
    );
}

#[test]
fn the_managed_runner_drives_the_handle_without_a_caller_reactor() {
    let (fake, _, dropped) = Fake::new();
    let mut managed = ManagedUsb::start(|registry, token, waker| {
        NativeUsb::attach(fake, session(), registry, token, waker).map_err(Error::Open)
    })
    .unwrap();
    managed.native().start_temperature(Instant::now()).unwrap();
    assert!(matches!(
        managed.next_event(GUARD).unwrap(),
        Some(NativeEvent::Temperature(Ok(celsius))) if (celsius - 25.0).abs() < f32::EPSILON
    ));
    managed.native().start_tx_open(4096, 4).unwrap();
    assert!(matches!(
        managed.next_event(GUARD).unwrap(),
        Some(NativeEvent::TxOpened(Ok(())))
    ));
    managed
        .native()
        .start_send(None, &[5; 256], GUARD, Instant::now())
        .unwrap();
    assert!(matches!(
        managed.next_event(GUARD).unwrap(),
        Some(NativeEvent::Sent(Ok(_)))
    ));
    assert!(
        managed.next_event(Duration::ZERO).unwrap().is_none(),
        "nothing else is in flight"
    );
    managed.close(GUARD).unwrap();
    assert_ne!(dropped.recv_timeout(GUARD).unwrap(), thread::current().id());
}

/// Waits through the reactor for the adapter's next stream outcome.
fn outcome(adapter: &mut Adapter, reactor: &mut Poll) -> StreamOutcome {
    let until = Instant::now() + GUARD;
    loop {
        if let Some(outcome) = adapter.try_outcome() {
            return outcome;
        }
        assert!(Instant::now() < until, "native stream outcome deadline");
        poll(adapter, reactor);
    }
}

/// Waits through the reactor for the adapter's next RX block.
fn block(adapter: &mut Adapter, reactor: &mut Poll) {
    let until = Instant::now() + GUARD;
    loop {
        if let Some(block) = adapter.try_block() {
            block.unwrap();
            return;
        }
        assert!(Instant::now() < until, "native RX block deadline");
        poll(adapter, reactor);
    }
}

#[test]
fn room_freed_after_a_turn_with_none_takes_the_blocks_that_turn_filed() {
    // A control transfer's turns keep reaping while the backlog is full: they file RX
    // completions no later edge announces, so freeing room must bring a turn by itself.
    let (_device, board) = rx_streaming(&[]);
    let (mut fake, _, _) = Fake::new();
    fake.io.board = board;
    fake.rx_filed = Some(0);
    let script = Arc::clone(&fake.io.script);
    let (mut reactor, wake) = reactor();
    let mut adapter = Adapter::attach(fake, reactor.registry(), DEVICE, wake, budget()).unwrap();
    // Consume the registration's own edge: from here, no device edge arrives.
    reactor
        .poll(&mut Events::with_capacity(8), Some(Duration::ZERO))
        .unwrap();
    adapter.command(StreamCommand::RxOpen(RX_BLOCK)).unwrap();
    assert!(matches!(
        outcome(&mut adapter, &mut reactor),
        StreamOutcome::Command(Ok(()))
    ));
    adapter.want_rx(true);
    adapter.submit(&operation(1).2, Instant::now()).unwrap();
    assert!(completion(&mut adapter, &mut reactor).result.is_ok());
    // The first read can leave a turn owed. The second runs on a full backlog: once its
    // acknowledgement is published, one turn (taking no block) pays what is owed and
    // brings its completion, and nothing more is owed.
    adapter.submit(&operation(2).2, Instant::now()).unwrap();
    reactor
        .poll(&mut Events::with_capacity(8), Some(GUARD))
        .unwrap();
    assert!(completion(&mut adapter, &mut reactor).result.is_ok());
    assert_eq!(
        script.lock().unwrap().submitted,
        8 + RX_BACKLOG,
        "the ring, then exactly the backlog's blocks taken and requeued"
    );
    for _ in 0..=RX_BACKLOG {
        block(&mut adapter, &mut reactor);
    }
    adapter.close().unwrap();
    cleanup(&mut adapter, &mut reactor).unwrap();
}

#[test]
fn a_failed_status_read_closes_the_handle_and_is_no_full_ring() {
    let (mut fake, _, dropped) = Fake::new();
    // TX opens without a control transfer: the first one is the first send's status read.
    fake.submit_error = Some(BusError::Timeout);
    let (mut reactor, wake) = reactor();
    let mut native = NativeUsb::attach(fake, session(), reactor.registry(), DEVICE, wake).unwrap();
    native.start_tx_open(4096, 4).unwrap();
    expect_event(
        &mut native,
        &mut reactor,
        |event| matches!(event, NativeEvent::TxOpened(Ok(()))),
        "TX opens",
    );
    native
        .start_send(None, &[1; 64], GUARD, Instant::now())
        .unwrap();
    expect_event(
        &mut native,
        &mut reactor,
        |event| {
            matches!(event, NativeEvent::Sent(Err(Error::Stream(BusError::Io(error))))
                if error.kind() == io::ErrorKind::TimedOut)
        },
        "a status-read timeout is an I/O failure, not Error::Timeout",
    );
    assert!(
        native.start_temperature(Instant::now()).is_err(),
        "a control-plane failure closes the handle"
    );
    native_cleanup(&mut native, &mut reactor).unwrap();
    assert_ne!(dropped.recv_timeout(GUARD).unwrap(), thread::current().id());
}

/// The gateware's share of a blocking trace, its register reads and writes: what the
/// native group performs on a handle without the board's RF state, which leaves out the
/// RF chains' power, calibration and tuning (LMS6002D SPI and clock I²C).
fn engine_operations(board: &Mutex<SimBoard>, start: usize) -> Vec<Op> {
    operations(board, start)
        .into_iter()
        .filter(|op| matches!(op, Op::RegWrite { .. } | Op::RegRead { .. }))
        .collect()
}

/// A 1024-sample `ci16` RX stream on channel 0.
fn rx_config() -> RxConfig {
    RxConfig::new(1024, HostFormat::Ci16, WireFormat::Ci16)
}

/// A 1024-sample TX stream from `ci16` host buffers.
fn tx_config() -> TxConfig {
    TxConfig::new(1024, HostFormat::Ci16, WireFormat::Ci16)
}

/// A tuned board's blocking device, and the board's trace length once tuned: the stream
/// group's create then finds an RX LO and tunes nothing itself.
fn tuned_device() -> (Device, Arc<Mutex<SimBoard>>, usize) {
    let (mut device, board, _) = simulated_device(BoardRevision::Rev3);
    device.set_rx_sample_rate(20_000_000).unwrap();
    device.set_rx_frequency(1_000_000_000).unwrap();
    let start = board.lock().unwrap().trace().len();
    (device, board, start)
}

#[test]
fn a_native_stream_group_writes_the_blocking_groups_engine_registers() {
    for config in [
        StreamsConfig::rx(rx_config()),
        StreamsConfig::tx(tx_config()),
        StreamsConfig::rx(rx_config())
            .with_tx(tx_config())
            .with_sync(SyncMode::All),
    ] {
        let (mut device, blocking, start) = tuned_device();
        device.create_streams(config).unwrap();
        let created = engine_operations(&blocking, start);
        let start = blocking.lock().unwrap().trace().len();
        device.start_streams().unwrap();
        let started = engine_operations(&blocking, start);
        let start = blocking.lock().unwrap().trace().len();
        device.stop_streams().unwrap();
        let stopped = engine_operations(&blocking, start);
        let hold = Op::RegWrite {
            addr: 14,
            value: 0x8000_0000,
        };
        assert!(
            created.contains(&hold) && !started.is_empty() && !stopped.is_empty(),
            "{config:?}: the blocking group wrote the engine (sync held, then released)"
        );

        let (mut reactor, wake) = reactor();
        let (mut native, board, _, _device) = powered(BoardRevision::Rev3, &reactor, wake);
        let start = board.lock().unwrap().trace().len();
        native.start_create_streams(config, Instant::now()).unwrap();
        expect_event(
            &mut native,
            &mut reactor,
            |event| matches!(event, NativeEvent::StreamsCreated(Ok(()))),
            "the group is created",
        );
        assert_eq!(
            operations(&board, start),
            created,
            "{config:?}: create writes the engine as the blocking group does"
        );
        let start = board.lock().unwrap().trace().len();
        native.start_streams(Instant::now()).unwrap();
        expect_event(
            &mut native,
            &mut reactor,
            |event| matches!(event, NativeEvent::StreamsStarted(Ok(()))),
            "the group starts",
        );
        assert_eq!(operations(&board, start), started, "{config:?}: start");
        if config.rx.is_some() {
            let NativeEvent::Received(Ok(block)) = finish(&mut native, &mut reactor) else {
                panic!("{config:?}: a started RX stream delivers blocks");
            };
            assert_eq!(block.payload.len(), 4096, "one 1024-sample ci16 packet");
        }
        let start = board.lock().unwrap().trace().len();
        native.start_stop_streams(Instant::now()).unwrap();
        let stopped_event = loop {
            match finish(&mut native, &mut reactor) {
                NativeEvent::Received(_) => {}
                event => break event,
            }
        };
        assert!(
            matches!(stopped_event, NativeEvent::StreamsStopped(Ok(()))),
            "{config:?}: the group stops: got {stopped_event:?}"
        );
        assert_eq!(operations(&board, start), stopped, "{config:?}: stop");
        native.close().unwrap();
        native_cleanup(&mut native, &mut reactor).unwrap();
    }
}

#[test]
fn native_create_refusals_happen_where_the_blocking_groups_do() {
    // An RX packet the burst plan cannot split fails after the DMA reset; a TX packet over
    // the core's FIFO after the gateware read and the sync hold.
    for (config, samples) in [
        (
            StreamsConfig::rx(RxConfig::new(0, HostFormat::Ci16, WireFormat::Ci16)),
            0,
        ),
        (
            StreamsConfig::tx(TxConfig::new(40_000, HostFormat::Ci16, WireFormat::Ci16)),
            40_000,
        ),
    ] {
        let refused =
            |error: &Error| matches!(error, Error::UnsupportedPacketSize(n) if *n == samples);
        let (mut device, blocking, start) = tuned_device();
        let blocking_error = device.create_streams(config).unwrap_err();
        assert!(refused(&blocking_error), "{config:?}: {blocking_error}");
        let created = engine_operations(&blocking, start);

        let (mut reactor, wake) = reactor();
        let (mut native, board, _, _device) = powered(BoardRevision::Rev3, &reactor, wake);
        let start = board.lock().unwrap().trace().len();
        native.start_create_streams(config, Instant::now()).unwrap();
        let NativeEvent::StreamsCreated(Err(error)) = finish(&mut native, &mut reactor) else {
            panic!("{config:?}: the create fails");
        };
        assert!(refused(&error), "{config:?}: the same refusal, got {error}");
        assert_eq!(
            operations(&board, start),
            created,
            "{config:?}: at the same point"
        );
        assert!(
            matches!(
                native.start_streams(Instant::now()),
                Err(Error::NotStreaming)
            ),
            "{config:?}: no stream joined the group"
        );
    }
}

#[test]
fn a_create_that_fails_after_rx_opened_keeps_rx_in_the_group_as_the_blocking_one_does() {
    let config = StreamsConfig::rx(rx_config())
        .with_tx(TxConfig::new(40_000, HostFormat::Ci16, WireFormat::Ci16))
        .with_sync(SyncMode::All);
    let (mut device, blocking, start) = tuned_device();
    assert!(matches!(
        device.create_streams(config),
        Err(Error::UnsupportedPacketSize(40_000))
    ));
    let created = engine_operations(&blocking, start);
    let start = blocking.lock().unwrap().trace().len();
    device.start_streams().unwrap();
    let started = engine_operations(&blocking, start);

    let (mut reactor, wake) = reactor();
    let (mut native, board, _, _device) = powered(BoardRevision::Rev3, &reactor, wake);
    let start = board.lock().unwrap().trace().len();
    native.start_create_streams(config, Instant::now()).unwrap();
    expect_event(
        &mut native,
        &mut reactor,
        |event| {
            matches!(
                event,
                NativeEvent::StreamsCreated(Err(Error::UnsupportedPacketSize(40_000)))
            )
        },
        "the TX packet is refused after RX opened",
    );
    assert_eq!(operations(&board, start), created, "the same create");
    let start = board.lock().unwrap().trace().len();
    native.start_streams(Instant::now()).unwrap();
    expect_event(
        &mut native,
        &mut reactor,
        |event| matches!(event, NativeEvent::StreamsStarted(Ok(()))),
        "the RX-only group starts",
    );
    assert_eq!(
        operations(&board, start),
        started,
        "SyncMode::All waits for RX alone, as the blocking group's start does"
    );
}

#[test]
fn a_dead_rx_dma_engine_fails_the_create_and_leaves_the_handle_open() {
    let (mut reactor, wake) = reactor();
    let (mut native, board, _, _device) = powered(BoardRevision::Rev3, &reactor, wake);
    // The RX DMA status as an absent or dead engine reads it.
    board.lock().unwrap().write_reg(6, 0xffff_ffff);
    let start = board.lock().unwrap().trace().len();
    native
        .start_create_streams(StreamsConfig::rx(rx_config()), Instant::now())
        .unwrap();
    expect_event(
        &mut native,
        &mut reactor,
        |event| {
            matches!(
                event,
                NativeEvent::StreamsCreated(Err(Error::DmaEngineFault))
            )
        },
        "the dead engine is reported as the blocking device reports it",
    );
    assert_eq!(
        operations(&board, start),
        [Op::RegRead {
            addr: 6,
            value: 0xffff_ffff
        }],
        "nothing follows the status read"
    );
    native.start_identity(Instant::now()).unwrap();
    expect_event(
        &mut native,
        &mut reactor,
        |event| matches!(event, NativeEvent::Identity(Ok(_))),
        "a fault found in good data keeps the handle open",
    );
}

#[test]
fn a_stop_attempts_every_step_after_a_failed_close() {
    let config = StreamsConfig::rx(rx_config())
        .with_tx(tx_config())
        .with_sync(SyncMode::All);
    let (mut reactor, wake) = reactor();
    let (_device, board, _) = simulated_device(BoardRevision::Rev3);
    let (mut fake, record, _) = Fake::new();
    fake.io.board = Arc::clone(&board);
    let script = Arc::clone(&fake.io.script);
    let mut native = NativeUsb::attach(fake, session(), reactor.registry(), DEVICE, wake).unwrap();
    native.start_create_streams(config, Instant::now()).unwrap();
    expect_event(
        &mut native,
        &mut reactor,
        |event| matches!(event, NativeEvent::StreamsCreated(Ok(()))),
        "the group is created",
    );
    script.lock().unwrap().fail_cancel = true;
    let start = board.lock().unwrap().trace().len();
    native.start_stop_streams(Instant::now()).unwrap();
    let stopped = loop {
        match finish(&mut native, &mut reactor) {
            NativeEvent::Received(_) => {}
            event => break event,
        }
    };
    assert!(
        matches!(
            stopped,
            NativeEvent::StreamsStopped(Err(Error::Stream(BusError::Timeout)))
        ),
        "the RX close's failure is reported: got {stopped:?}"
    );
    let tx_stop = Op::RegWrite { addr: 14, value: 0 };
    assert_eq!(
        operations(&board, start).last(),
        Some(&tx_stop),
        "TX's destroy still stopped its core after RX's close failed"
    );
    assert!(
        calls(&record, "tx close").len() == 1,
        "TX's transport still closed"
    );
    assert!(
        matches!(
            native.start_streams(Instant::now()),
            Err(Error::NotStreaming)
        ),
        "the group is gone"
    );
    script.lock().unwrap().fail_cancel = false;
    native.start_rx_close().unwrap();
    expect_event(
        &mut native,
        &mut reactor,
        |event| matches!(event, NativeEvent::RxClosed(Ok(()))),
        "the held RX stream closes outside the group",
    );
}

#[test]
fn group_operations_refuse_before_any_io_as_the_blocking_group_does() {
    let (mut reactor, wake) = reactor();
    let (mut native, board, record, _device) = powered(BoardRevision::Rev3, &reactor, wake);
    let start = board.lock().unwrap().trace().len();
    assert!(matches!(
        native.start_streams(Instant::now()),
        Err(Error::NotStreaming)
    ));
    assert!(matches!(
        native.start_stop_streams(Instant::now()),
        Err(Error::NotStreaming)
    ));
    assert!(matches!(
        native.start_rx_ready(Instant::now()),
        Err(Error::NotStreaming)
    ));
    let none = StreamsConfig::rx(rx_config());
    let none = StreamsConfig { rx: None, ..none };
    assert!(matches!(
        native.start_create_streams(none, Instant::now()),
        Err(Error::NoStreams)
    ));
    let external = StreamsConfig::rx(rx_config()).with_sync(SyncMode::ExternalAll);
    assert!(matches!(
        native.start_create_streams(external, Instant::now()),
        Err(Error::UnsupportedSync(SyncMode::ExternalAll))
    ));
    let twelve = StreamsConfig::tx(TxConfig::new(1024, HostFormat::Ci16, WireFormat::Ci12));
    assert!(matches!(
        native.start_create_streams(twelve, Instant::now()),
        Err(Error::UnsupportedWireFormat(WireFormat::Ci12))
    ));
    assert_eq!(operations(&board, start), [], "nothing reached the board");
    assert_eq!(submits(&record), 0, "nothing reached the worker");

    native
        .start_create_streams(StreamsConfig::rx(rx_config()), Instant::now())
        .unwrap();
    assert!(matches!(
        native.start_streams(Instant::now()),
        Err(Error::Busy)
    ));
    expect_event(
        &mut native,
        &mut reactor,
        |event| matches!(event, NativeEvent::StreamsCreated(Ok(()))),
        "the group is created",
    );
    assert!(matches!(
        native.start_create_streams(StreamsConfig::rx(rx_config()), Instant::now()),
        Err(Error::AlreadyStreaming)
    ));
    assert!(
        matches!(native.start_rx_close(), Err(Error::AlreadyStreaming)),
        "the group's streams close with the group"
    );
    native.start_rx_ready(Instant::now()).unwrap();
    expect_event(
        &mut native,
        &mut reactor,
        |event| matches!(event, NativeEvent::RxReady(Ok(()))),
        "the host-ready write",
    );
    assert_eq!(
        operations(&board, 0).last(),
        Some(&Op::RegWrite { addr: 5, value: 4 }),
        "the DMA control register's ready bit, as Device::set_rx_ready writes it"
    );
}

/// Drives the handle until its pending operation ends, passing over the RX blocks a
/// started group delivers meanwhile.
fn settle(native: &mut NativeUsb, reactor: &mut Poll) -> NativeEvent {
    loop {
        match finish(native, reactor) {
            NativeEvent::Received(_) => {}
            event => return event,
        }
    }
}

/// The board's trace length: where the next operation's ops start.
fn mark(board: &Mutex<SimBoard>) -> usize {
    board.lock().unwrap().trace().len()
}

#[test]
fn native_mute_and_throttle_write_what_the_blocking_device_writes() {
    let config = StreamsConfig::rx(rx_config())
        .with_tx(tx_config())
        .with_sync(SyncMode::All);
    let throttle = Some(RxThrottle { send: 1, skip: 2 });
    let (mut device, blocking, _) = tuned_device();
    device.create_streams(config).unwrap();
    let start = mark(&blocking);
    device.set_tx_mute(TxMute::Neither).unwrap();
    let unmuted = engine_operations(&blocking, start);
    let start = mark(&blocking);
    device.start_streams().unwrap();
    let started = engine_operations(&blocking, start);
    let start = mark(&blocking);
    device.set_rx_throttle(throttle).unwrap();
    let throttled = engine_operations(&blocking, start);
    let start = mark(&blocking);
    device.set_rx_throttle(None).unwrap();
    let released = engine_operations(&blocking, start);
    let start = mark(&blocking);
    device.set_tx_mute(TxMute::Both).unwrap();
    let muted = engine_operations(&blocking, start);

    let (mut reactor, wake) = reactor();
    let (mut native, board, _, _device) = powered(BoardRevision::Rev3, &reactor, wake);
    native.start_create_streams(config, Instant::now()).unwrap();
    assert!(matches!(
        settle(&mut native, &mut reactor),
        NativeEvent::StreamsCreated(Ok(()))
    ));
    let start = mark(&board);
    native
        .start_tx_mute(TxMute::Neither, Instant::now())
        .unwrap();
    assert!(matches!(
        settle(&mut native, &mut reactor),
        NativeEvent::TxMuted(Ok(()))
    ));
    assert_eq!(
        operations(&board, start),
        unmuted,
        "a mute before the start"
    );
    let start = mark(&board);
    native.start_streams(Instant::now()).unwrap();
    assert!(matches!(
        settle(&mut native, &mut reactor),
        NativeEvent::StreamsStarted(Ok(()))
    ));
    assert_eq!(
        operations(&board, start),
        started,
        "the start applies the mute the group kept, as the blocking start does"
    );
    let start = mark(&board);
    native.start_rx_throttle(throttle, Instant::now()).unwrap();
    assert!(matches!(
        settle(&mut native, &mut reactor),
        NativeEvent::RxThrottled(Ok(()))
    ));
    assert_eq!(operations(&board, start), throttled, "a throttle");
    let start = mark(&board);
    native.start_rx_throttle(None, Instant::now()).unwrap();
    assert!(matches!(
        settle(&mut native, &mut reactor),
        NativeEvent::RxThrottled(Ok(()))
    ));
    assert_eq!(operations(&board, start), released, "the throttle released");
    let start = mark(&board);
    native.start_tx_mute(TxMute::Both, Instant::now()).unwrap();
    assert!(matches!(
        settle(&mut native, &mut reactor),
        NativeEvent::TxMuted(Ok(()))
    ));
    assert_eq!(operations(&board, start), muted, "a mute while running");
}

#[test]
fn mute_and_throttle_refuse_before_any_io_without_their_stream() {
    let (mut reactor, wake) = reactor();
    let (mut native, board, record, _device) = powered(BoardRevision::Rev3, &reactor, wake);
    assert!(matches!(
        native.start_tx_mute(TxMute::A, Instant::now()),
        Err(Error::NotStreaming)
    ));
    assert!(matches!(
        native.start_rx_throttle(None, Instant::now()),
        Err(Error::NotStreaming)
    ));
    native
        .start_create_streams(StreamsConfig::tx(tx_config()), Instant::now())
        .unwrap();
    assert!(matches!(
        native.start_tx_mute(TxMute::A, Instant::now()),
        Err(Error::Busy)
    ));
    assert!(matches!(
        settle(&mut native, &mut reactor),
        NativeEvent::StreamsCreated(Ok(()))
    ));
    let (start, submitted) = (mark(&board), submits(&record));
    assert!(
        matches!(
            native.start_rx_throttle(None, Instant::now()),
            Err(Error::NotStreaming)
        ),
        "a TX-only group has no RX stream to throttle, as on the blocking device"
    );
    assert_eq!(operations(&board, start), [], "nothing reached the board");
    assert_eq!(submits(&record), submitted, "nothing reached the worker");
}

/// Forwards a handed-over transport, so the native fake drives the very board model the
/// blocking bus used, its notification counter and I²C table included.
impl UsbIo for Box<dyn UsbIo> {
    fn write(&mut self, packet: &[u8], timeout: Duration) -> Result<(), BusError> {
        (**self).write(packet, timeout)
    }

    fn read(&mut self, reply: Endpoint, timeout: Duration) -> Result<Vec<u8>, BusError> {
        (**self).read(reply, timeout)
    }

    fn rx_open(&mut self, block_bytes: u32) -> Result<(), BusError> {
        (**self).rx_open(block_bytes)
    }

    fn rx_recv(
        &mut self,
        timeout: Duration,
        consume: &mut dyn FnMut(&[u8], [u64; 2]),
    ) -> Result<(), BusError> {
        (**self).rx_recv(timeout, consume)
    }

    fn rx_close(&mut self) -> Result<(), BusError> {
        (**self).rx_close()
    }

    fn tx_open(&mut self, block_bytes: u32, sample_bytes: u32) -> Result<(), BusError> {
        (**self).tx_open(block_bytes, sample_bytes)
    }

    fn tx_acquire(&mut self, timeout: Duration) -> Result<(), BusError> {
        (**self).tx_acquire(timeout)
    }

    fn tx_commit(
        &mut self,
        time: Option<u64>,
        status: Option<[u32; 4]>,
        fill: &mut TxFill<'_>,
    ) -> Result<(), BusError> {
        (**self).tx_commit(time, status, fill)
    }

    fn tx_close(&mut self) -> Result<(), BusError> {
        (**self).tx_close()
    }

    fn holds_stream(&self) -> bool {
        (**self).holds_stream()
    }
}

/// A device set up with the blocking API: both chains' rate, tuning and IQ corrections,
/// the corrections off their defaults so a stream create's DSP writes show them.
fn set_up_device() -> (Device, Arc<Mutex<SimBoard>>) {
    let (mut device, board, _) = simulated_device(BoardRevision::Rev3);
    device.set_sample_rate(20_000_000).unwrap();
    device.set_rx_frequency(1_000_000_000).unwrap();
    device.set_tx_frequency(1_200_000_000).unwrap();
    device
        .set_rx_iq_imbalance(IqImbalance {
            amplitude: 30_000,
            phase: -2_000,
        })
        .unwrap();
    device
        .set_tx_iq_imbalance(IqImbalance {
            amplitude: -12_000,
            phase: 1_500,
        })
        .unwrap();
    (device, board)
}

#[test]
fn a_handed_over_device_streams_as_the_blocking_device_does() {
    let config = StreamsConfig::rx(rx_config())
        .with_tx(tx_config())
        .with_sync(SyncMode::All);
    let (mut device, blocking) = set_up_device();
    let start = mark(&blocking);
    device.create_streams(config).unwrap();
    let created = operations(&blocking, start);
    let start = mark(&blocking);
    device.start_streams().unwrap();
    let started = operations(&blocking, start);
    let start = mark(&blocking);
    device.stop_streams().unwrap();
    let stopped = operations(&blocking, start);

    let (mut device, board) = set_up_device();
    let start = mark(&board);
    let handed = device.hand_over_usb().unwrap();
    drop(device);
    assert_eq!(
        operations(&board, start),
        [],
        "the hand-over performs no I/O, and the device leaves the board powered"
    );
    let (mut reactor, wake) = reactor();
    let mut native = handed
        .attach_with(|io| Ok(Fake::over(io).0), reactor.registry(), DEVICE, wake)
        .unwrap();
    native.start_temperature(Instant::now()).unwrap();
    assert!(
        reading(&mut native, &mut reactor).is_ok(),
        "the handle continues the device's notification sequence and I2C table"
    );
    let start = mark(&board);
    native.start_create_streams(config, Instant::now()).unwrap();
    assert!(matches!(
        settle(&mut native, &mut reactor),
        NativeEvent::StreamsCreated(Ok(()))
    ));
    assert_eq!(
        operations(&board, start),
        created,
        "the create performs the blocking create's operations, the RF chains' power, DC \
         calibration and TX retune from the carried RF state and the carried IQ \
         corrections included"
    );
    let start = mark(&board);
    native.start_streams(Instant::now()).unwrap();
    assert!(matches!(
        settle(&mut native, &mut reactor),
        NativeEvent::StreamsStarted(Ok(()))
    ));
    assert_eq!(operations(&board, start), started, "the start");
    let start = mark(&board);
    native.start_stop_streams(Instant::now()).unwrap();
    assert!(matches!(
        settle(&mut native, &mut reactor),
        NativeEvent::StreamsStopped(Ok(()))
    ));
    assert_eq!(
        operations(&board, start),
        stopped,
        "the stop, the chains powered down"
    );
    native.close().unwrap();
    native_cleanup(&mut native, &mut reactor).unwrap();
}

#[test]
fn a_device_with_a_stream_group_keeps_its_bus() {
    let (mut device, board, _) = tuned_device();
    device
        .create_streams(StreamsConfig::rx(rx_config()))
        .unwrap();
    let start = mark(&board);
    assert!(matches!(
        device.hand_over_usb(),
        Err(Error::AlreadyStreaming)
    ));
    assert_eq!(operations(&board, start), [], "refused before any I/O");
    device
        .stop_streams()
        .expect("the device still holds its bus and group");
}

#[test]
fn a_device_holding_a_failed_streams_endpoint_is_refused_and_powered_down() {
    let (mut device, board, script) = simulated_device(BoardRevision::Rev3);
    device.set_rx_sample_rate(20_000_000).unwrap();
    device.set_rx_frequency(1_000_000_000).unwrap();
    device
        .create_streams(StreamsConfig::rx(rx_config()))
        .unwrap();
    script.lock().unwrap().fail_cancel = true;
    assert!(
        device.stop_streams().is_err(),
        "the RX close fails, so the bus keeps the endpoint"
    );
    let (reactor, wake) = reactor();
    let refused = device.into_native(reactor.registry(), DEVICE, wake);
    assert!(
        matches!(refused, Err(Error::Open(BusError::Unsupported(_)))),
        "a held endpoint is refused before the bus is given up: {refused:?}"
    );
    assert_eq!(
        board.lock().unwrap().gpo(0),
        Some(0),
        "the refused device still powered the board down (LMS6002D reset)"
    );
}

/// A fresh board of `revision`, warmed to `millicelsius`, on a USB bus whose router is set
/// up, as `simulated_device` makes it. Both PLLs fail to lock below 245 MHz, as the
/// oracle's low-LO fallback scenarios model it.
fn usb_board(revision: BoardRevision, millicelsius: i32) -> (UsbBus, Arc<Mutex<SimBoard>>) {
    let lock = PllLock {
        unlocked_below_hz: 245_000_000,
        ..PllLock::default()
    };
    let mut board = SimBoard::new(revision)
        .with_temperature(millicelsius)
        .with_rx_pll_lock(lock)
        .with_tx_pll_lock(lock);
    let hwid = board.read_reg(19);
    board.write_reg(19, hwid | 0x4000_0000);
    let board = Arc::new(Mutex::new(board));
    let io = BoardUsb {
        script: Arc::default(),
        rx: None,
        tx: None,
        board: Arc::clone(&board),
        registers: VecDeque::new(),
        notifications: VecDeque::new(),
        sequence: 0,
        lut: 0,
        index: 0,
    };
    let mut bus = UsbBus::from_io(Box::new(io));
    bus.initialize().unwrap();
    (bus, board)
}

/// The blocking device on a fresh board, opened without the front-end probe, as a native
/// power-up runs.
fn unprobed_device(revision: BoardRevision) -> (Device, Arc<Mutex<SimBoard>>) {
    let (bus, board) = usb_board(revision, 25_000);
    let device = Device::builder(bus)
        .front_end(FrontEndSelection::None)
        .open()
        .unwrap();
    (device, board)
}

/// A native handle over a fresh board of `revision` at `millicelsius`, identified and
/// unpowered: `UsbBus::into_native`'s hand-over, attached to the whole-transfer fake.
fn unpowered_native(
    revision: BoardRevision,
    millicelsius: i32,
    reactor: &Poll,
    wake: Arc<Waker>,
) -> (NativeUsb, Arc<Mutex<SimBoard>>) {
    let (bus, board) = usb_board(revision, millicelsius);
    let native = bus
        .hand_over()
        .unwrap()
        .attach_with(|io| Ok(Fake::over(io).0), reactor.registry(), DEVICE, wake)
        .unwrap();
    (native, board)
}

/// One RF setting, made on either path.
#[derive(Clone, Copy, Debug)]
enum Setting {
    /// Both chains' sample rate.
    Rate(u32),
    /// The RX sample rate alone.
    RxRate(u32),
    /// The RX frequency.
    RxHz(u32),
    /// The TX frequency.
    TxHz(u32),
    /// An RX gain.
    RxGain(RxGain),
    /// A TX gain.
    TxGain(TxGain),
    /// The RX path.
    RxPath(RxPath),
    /// The TX path.
    TxPath(TxPath),
    /// The RX bandwidth.
    RxBandwidth(u32),
    /// The TX bandwidth.
    TxBandwidth(u32),
}

impl Setting {
    /// Makes the setting on the blocking device.
    fn on_device(self, device: &mut Device) -> Result<(), Error> {
        match self {
            Self::Rate(rate) => device.set_sample_rate(rate),
            Self::RxRate(rate) => device.set_rx_sample_rate(rate),
            Self::RxHz(hz) => device.set_rx_frequency(hz),
            Self::TxHz(hz) => device.set_tx_frequency(hz),
            Self::RxGain(gain) => device.set_rx_gain(gain),
            Self::TxGain(gain) => device.set_tx_gain(gain),
            Self::RxPath(path) => device.set_rx_path(path),
            Self::TxPath(path) => device.set_tx_path(path),
            Self::RxBandwidth(hz) => device.set_rx_bandwidth(hz),
            Self::TxBandwidth(hz) => device.set_tx_bandwidth(hz),
        }
    }

    /// Makes the setting on the native handle and drives it to its event's result.
    fn on_native(self, native: &mut NativeUsb, reactor: &mut Poll) -> Result<(), Error> {
        let now = Instant::now();
        match self {
            Self::Rate(rate) => native.start_sample_rate(rate, now),
            Self::RxRate(rate) => native.start_rx_sample_rate(rate, now),
            Self::RxHz(hz) => native.start_rx_frequency(hz, now),
            Self::TxHz(hz) => native.start_tx_frequency(hz, now),
            Self::RxGain(gain) => native.start_rx_gain(gain, now),
            Self::TxGain(gain) => native.start_tx_gain(gain, now),
            Self::RxPath(path) => native.start_rx_path(path, now),
            Self::TxPath(path) => native.start_tx_path(path, now),
            Self::RxBandwidth(hz) => native.start_rx_bandwidth(hz, now),
            Self::TxBandwidth(hz) => native.start_tx_bandwidth(hz, now),
        }?;
        match settle(native, reactor) {
            NativeEvent::SampleRateSet(result)
            | NativeEvent::Tuned(result)
            | NativeEvent::GainSet(result)
            | NativeEvent::PathSet(result)
            | NativeEvent::BandwidthSet(result) => result,
            other => panic!("{self:?}: a setting's event, got {other:?}"),
        }
    }
}

/// Whether two results are the same: both done, or both failed with the same error kind.
fn same_outcome(blocking: &Result<(), Error>, native: &Result<(), Error>) -> bool {
    match (blocking, native) {
        (Ok(()), Ok(())) => true,
        (Err(blocking), Err(native)) => mem::discriminant(blocking) == mem::discriminant(native),
        _ => false,
    }
}

#[test]
fn a_native_power_up_setup_and_stream_perform_the_blocking_devices_operations() {
    let settings = [
        Setting::Rate(1_000_000),
        // Below 250 MHz without a lock: the low-LO sweep and its NCO fallback, then the
        // minimal LO found reused.
        Setting::RxHz(240_000_000),
        Setting::RxHz(241_000_000),
        Setting::TxHz(240_000_000),
        // At 20 MS/s the NCO offset no longer fits: the fallback fails on both paths.
        Setting::Rate(20_000_000),
        Setting::TxHz(242_000_000),
        // The mixer band: the board mixer on and the Si5332's output 3 gated.
        Setting::RxHz(100_000_000),
        Setting::TxHz(1_200_000_000),
        Setting::RxHz(900_000_000),
        Setting::RxGain(RxGain::Auto(40)),
        Setting::RxGain(RxGain::Vga2b(6)),
        Setting::TxGain(TxGain::Vga1(-12)),
        Setting::TxGain(TxGain::Vga2(10)),
        Setting::RxPath(RxPath::WideLoopback),
        Setting::RxPath(RxPath::Auto),
        Setting::TxPath(TxPath::High),
        Setting::TxPath(TxPath::Auto),
        Setting::RxBandwidth(5_000_000),
        Setting::TxBandwidth(0),
        // A rate in the mixer band retunes the receiver; the RX rate alone reloads only its
        // FIR.
        Setting::RxHz(150_000_000),
        Setting::RxRate(5_000_000),
    ];
    for revision in [BoardRevision::Rev1, BoardRevision::Rev3] {
        let (mut device, blocking) = unprobed_device(revision);
        let (mut reactor, wake) = reactor();
        let (mut native, board) = unpowered_native(revision, 25_000, &reactor, wake);
        native
            .start_power_up(
                ReferenceClock::Board,
                ThermalPolicy::WithinSpec,
                Instant::now(),
            )
            .unwrap();
        assert!(matches!(
            settle(&mut native, &mut reactor),
            NativeEvent::PoweredUp(Ok(()))
        ));
        assert_eq!(
            operations(&board, 0),
            operations(&blocking, 0),
            "{revision:?}: identification, the thermal reading and power-up, op for op"
        );
        for setting in settings {
            let (blocking_start, native_start) = (mark(&blocking), mark(&board));
            let expected = setting.on_device(&mut device);
            let found = setting.on_native(&mut native, &mut reactor);
            assert!(
                same_outcome(&expected, &found),
                "{revision:?} {setting:?}: blocking {expected:?}, native {found:?}"
            );
            assert_eq!(
                found.is_err(),
                matches!(setting, Setting::TxHz(242_000_000)),
                "{revision:?} {setting:?}: only the fallback without NCO room fails"
            );
            let expected = operations(&blocking, blocking_start);
            assert!(
                !expected.is_empty(),
                "{revision:?} {setting:?} reached the board"
            );
            assert_eq!(
                operations(&board, native_start),
                expected,
                "{revision:?} {setting:?}"
            );
        }
        stream_then_power_down(
            revision,
            (device, &blocking),
            (&mut native, &board),
            &mut reactor,
        );
        native.close().unwrap();
        native_cleanup(&mut native, &mut reactor).unwrap();
    }
}

/// Creates, starts and stops an RX and TX stream group, then powers the board down, on
/// both paths, comparing each step's operations; the native handle also receives a block.
fn stream_then_power_down(
    revision: BoardRevision,
    (mut device, blocking): (Device, &Mutex<SimBoard>),
    (native, board): (&mut NativeUsb, &Mutex<SimBoard>),
    reactor: &mut Poll,
) {
    let config = StreamsConfig::rx(rx_config())
        .with_tx(tx_config())
        .with_sync(SyncMode::All);
    let (blocking_start, native_start) = (mark(blocking), mark(board));
    device.create_streams(config).unwrap();
    native.start_create_streams(config, Instant::now()).unwrap();
    assert!(matches!(
        settle(native, reactor),
        NativeEvent::StreamsCreated(Ok(()))
    ));
    assert_eq!(
        operations(board, native_start),
        operations(blocking, blocking_start),
        "{revision:?}: the create, the RX DC calibration and TX retune included"
    );
    native.start_streams(Instant::now()).unwrap();
    assert!(matches!(
        settle(native, reactor),
        NativeEvent::StreamsStarted(Ok(()))
    ));
    let NativeEvent::Received(Ok(block)) = finish(native, reactor) else {
        panic!("{revision:?}: a natively powered and tuned board streams");
    };
    assert_eq!(block.payload.len(), 4096);
    device.start_streams().unwrap();
    let (blocking_start, native_start) = (mark(blocking), mark(board));
    device.stop_streams().unwrap();
    native.start_stop_streams(Instant::now()).unwrap();
    assert!(matches!(
        settle(native, reactor),
        NativeEvent::StreamsStopped(Ok(()))
    ));
    assert_eq!(
        operations(board, native_start),
        operations(blocking, blocking_start),
        "{revision:?}: the stop, the chains powered down"
    );
    let (blocking_start, native_start) = (mark(blocking), mark(board));
    device.close().unwrap();
    native.start_power_down(Instant::now()).unwrap();
    assert!(matches!(
        settle(native, reactor),
        NativeEvent::PoweredDown(Ok(()))
    ));
    assert_eq!(
        operations(board, native_start),
        operations(blocking, blocking_start),
        "{revision:?}: the power-down"
    );
    assert!(
        matches!(
            native.start_rx_frequency(1_000_000_000, Instant::now()),
            Err(Error::NotPowered)
        ),
        "a powered-down handle holds no RF state"
    );
}

#[test]
fn a_native_power_up_refuses_a_board_too_hot_for_its_policy_before_powering_anything() {
    for (millicelsius, policy) in [
        (80_000, ThermalPolicy::WithinSpec),
        (110_000, ThermalPolicy::HardStopOnly),
    ] {
        let (mut reactor, wake) = reactor();
        let (mut native, board) =
            unpowered_native(BoardRevision::Rev3, millicelsius, &reactor, wake);
        let start = mark(&board);
        native
            .start_power_up(ReferenceClock::Board, policy, Instant::now())
            .unwrap();
        let NativeEvent::PoweredUp(Err(Error::Overheated { celsius, limit })) =
            settle(&mut native, &mut reactor)
        else {
            panic!("{policy:?}: a board at its start limit is refused");
        };
        assert!(
            celsius >= limit,
            "{policy:?}: {celsius} °C against {limit} °C"
        );
        let ops = operations(&board, start);
        assert!(
            matches!(ops.as_slice(), [Op::I2c { .. }]),
            "{policy:?}: only the temperature read reached the board: {ops:?}"
        );
        assert!(
            matches!(
                native.start_sample_rate(20_000_000, Instant::now()),
                Err(Error::NotPowered)
            ),
            "{policy:?}: the handle stays open and unpowered"
        );
        native.close().unwrap();
        native_cleanup(&mut native, &mut reactor).unwrap();
    }
    // Once powered, a reading at the policy's stop limit is reported as overheating, and
    // the handle stays open.
    let (mut powered_reactor, wake) = reactor();
    let (mut native, board) = unpowered_native(BoardRevision::Rev3, 25_000, &powered_reactor, wake);
    native
        .start_power_up(
            ReferenceClock::Board,
            ThermalPolicy::WithinSpec,
            Instant::now(),
        )
        .unwrap();
    assert!(matches!(
        settle(&mut native, &mut powered_reactor),
        NativeEvent::PoweredUp(Ok(()))
    ));
    board.lock().unwrap().set_temperature(85_000);
    native.start_temperature(Instant::now()).unwrap();
    assert!(matches!(
        reading(&mut native, &mut powered_reactor),
        Err(Error::Overheated { limit, .. }) if (limit - 85.0).abs() < f32::EPSILON
    ));
    board.lock().unwrap().set_temperature(84_000);
    native.start_temperature(Instant::now()).unwrap();
    assert!(
        reading(&mut native, &mut powered_reactor).is_ok(),
        "below the stop limit"
    );
    native.close().unwrap();
    native_cleanup(&mut native, &mut powered_reactor).unwrap();
    // Below the hard stop, the hard-stop-only policy powers up.
    let (mut reactor, wake) = reactor();
    let (mut native, _) = unpowered_native(BoardRevision::Rev3, 105_000, &reactor, wake);
    native
        .start_power_up(
            ReferenceClock::Board,
            ThermalPolicy::HardStopOnly,
            Instant::now(),
        )
        .unwrap();
    assert!(matches!(
        settle(&mut native, &mut reactor),
        NativeEvent::PoweredUp(Ok(()))
    ));
    native.close().unwrap();
    native_cleanup(&mut native, &mut reactor).unwrap();
}

#[test]
fn rf_settings_refuse_before_any_io_without_a_power_up_and_power_up_refuses_twice() {
    let (mut reactor, wake) = reactor();
    let (mut native, board) = unpowered_native(BoardRevision::Rev3, 25_000, &reactor, wake);
    let start = mark(&board);
    let now = Instant::now();
    assert!(matches!(
        native.start_sample_rate(20_000_000, now),
        Err(Error::NotPowered)
    ));
    assert!(matches!(
        native.start_tx_frequency(1_000_000_000, now),
        Err(Error::NotPowered)
    ));
    assert!(matches!(
        native.start_rx_gain(RxGain::Lna(6), now),
        Err(Error::NotPowered)
    ));
    assert!(matches!(
        native.start_tx_path(TxPath::High, now),
        Err(Error::NotPowered)
    ));
    assert!(matches!(
        native.start_power_down(now),
        Err(Error::NotPowered)
    ));
    assert_eq!(operations(&board, start), [], "refused before any I/O");
    native
        .start_power_up(ReferenceClock::Board, ThermalPolicy::default(), now)
        .unwrap();
    assert!(matches!(
        native.start_power_up(ReferenceClock::Board, ThermalPolicy::default(), now),
        Err(Error::Busy)
    ));
    assert!(matches!(
        settle(&mut native, &mut reactor),
        NativeEvent::PoweredUp(Ok(()))
    ));
    assert!(matches!(
        native.start_power_up(ReferenceClock::Board, ThermalPolicy::default(), now),
        Err(Error::AlreadyPowered)
    ));
    assert!(matches!(
        native.start_sample_rate(1, Instant::now()),
        Err(Error::UnsupportedSampleRate(1))
    ));
    native.close().unwrap();
    native_cleanup(&mut native, &mut reactor).unwrap();
}

/// A native handle over a fresh board, powered up natively, with an RX and TX stream group
/// created and started, every event so far drained.
fn streaming_native(reactor: &mut Poll, wake: Arc<Waker>) -> (NativeUsb, Arc<Mutex<SimBoard>>) {
    let (mut native, board) = unpowered_native(BoardRevision::Rev3, 25_000, reactor, wake);
    native
        .start_power_up(
            ReferenceClock::Board,
            ThermalPolicy::WithinSpec,
            Instant::now(),
        )
        .unwrap();
    assert!(matches!(
        settle(&mut native, reactor),
        NativeEvent::PoweredUp(Ok(()))
    ));
    for setting in [Setting::Rate(20_000_000), Setting::RxHz(1_000_000_000)] {
        setting.on_native(&mut native, reactor).unwrap();
    }
    let config = StreamsConfig::rx(rx_config()).with_tx(tx_config());
    native.start_create_streams(config, Instant::now()).unwrap();
    assert!(matches!(
        settle(&mut native, reactor),
        NativeEvent::StreamsCreated(Ok(()))
    ));
    native.start_streams(Instant::now()).unwrap();
    assert!(matches!(
        settle(&mut native, reactor),
        NativeEvent::StreamsStarted(Ok(()))
    ));
    (native, board)
}

#[test]
fn control_operations_complete_beside_a_saturated_rx_stream_and_rx_flows_during_them() {
    let (mut reactor, wake) = reactor();
    let (mut native, _board) = streaming_native(&mut reactor, wake);
    // RX blocks are left unread, so the backlog fills and the worker's turns take none:
    // the control transfers' completions still arrive through those turns.
    for _ in 0..3 {
        native.start_temperature(Instant::now()).unwrap();
        assert!(
            reading(&mut native, &mut reactor).is_ok(),
            "a control operation completes while RX holds a full backlog"
        );
    }
    // A long operation (a retune: PLL search, NCOs and filter, dozens of transfers and
    // waits) runs while RX keeps delivering: each poll advances the operation first,
    // then hands over an RX block.
    native
        .start_rx_frequency(1_100_000_000, Instant::now())
        .unwrap();
    let mut blocks = 0;
    let tuned = loop {
        match finish(&mut native, &mut reactor) {
            NativeEvent::Received(Ok(_)) => blocks += 1,
            event => break event,
        }
    };
    assert!(matches!(tuned, NativeEvent::Tuned(Ok(()))), "{tuned:?}");
    assert!(
        blocks > RX_BACKLOG,
        "RX blocks flowed during the operation, not only the backlog held before it: {blocks}"
    );
    native.start_stop_streams(Instant::now()).unwrap();
    assert!(matches!(
        settle(&mut native, &mut reactor),
        NativeEvent::StreamsStopped(Ok(()))
    ));
    native.close().unwrap();
    native_cleanup(&mut native, &mut reactor).unwrap();
}

#[test]
fn an_idle_devices_hang_up_ends_the_session_once_and_releases_it_on_the_worker() {
    for panic in [false, true] {
        let (mut fake, record, dropped) = Fake::new();
        if panic {
            record.lock().unwrap().panic_turn = true;
        } else {
            fake.turn_error = Some(BusError::Disconnected);
        }
        let (mut reactor, wake) = reactor();
        let mut native =
            NativeUsb::attach(fake, session(), reactor.registry(), DEVICE, wake).unwrap();
        // Nothing is in flight: the device's edge (its hang-up) brings a reap turn.
        native.on_ready(DEVICE);
        let ended = finish(&mut native, &mut reactor);
        assert!(
            matches!(ended, NativeEvent::SessionEnded(BusError::Disconnected)),
            "panic {panic}: {ended:?}"
        );
        assert!(
            native.poll(Instant::now()).is_none(),
            "panic {panic}: the end is reported once"
        );
        assert!(
            native.start_temperature(Instant::now()).is_err(),
            "panic {panic}: the handle is closed"
        );
        native_cleanup(&mut native, &mut reactor).unwrap();
        let worker = dropped.recv_timeout(GUARD).unwrap();
        assert_ne!(worker, thread::current().id());
        assert!(
            calls(&record, "cleanup")
                .iter()
                .all(|thread| *thread == worker),
            "panic {panic}: the worker released the device"
        );
    }
}

#[test]
fn a_worker_panic_while_streaming_fails_the_stream_once_and_releases_it() {
    let (mut reactor, wake) = reactor();
    let (bus, _) = usb_board(BoardRevision::Rev3, 25_000);
    let mut panicking = None;
    let mut native = bus
        .hand_over()
        .unwrap()
        .attach_with(
            |io| {
                let (fake, record, dropped) = Fake::over(io);
                panicking = Some((record, dropped));
                Ok(fake)
            },
            reactor.registry(),
            DEVICE,
            wake,
        )
        .unwrap();
    let (record, dropped) = panicking.unwrap();
    native.start_rx_open(RX_BLOCK).unwrap();
    expect_event(
        &mut native,
        &mut reactor,
        |event| matches!(event, NativeEvent::RxOpened(Ok(()))),
        "RX opens",
    );
    record.lock().unwrap().panic_turn = true;
    native.on_ready(DEVICE);
    expect_event(
        &mut native,
        &mut reactor,
        |event| {
            matches!(
                event,
                NativeEvent::Received(Err(Error::Stream(BusError::Disconnected)))
            )
        },
        "the stream reports the worker's failure",
    );
    assert!(
        native.poll(Instant::now()).is_none(),
        "the stream carried the session's end: nothing more is reported"
    );
    native_cleanup(&mut native, &mut reactor).unwrap();
    assert_ne!(dropped.recv_timeout(GUARD).unwrap(), thread::current().id());
}

/// Unparks the test thread and counts the wakes: a bare executor, no runtime.
struct ThreadWake {
    /// The thread polling the future.
    thread: thread::Thread,
    /// Wakes so far.
    wakes: AtomicUsize,
}

impl Wake for ThreadWake {
    fn wake(self: Arc<Self>) {
        self.wakes.fetch_add(1, Ordering::SeqCst);
        self.thread.unpark();
    }
}

/// Polls `future` on this thread until it resolves, parking between wakes; its output,
/// whether its first poll was pending, and the wakes it took.
fn block_on<F: Future>(future: F) -> (F::Output, bool, usize) {
    let wake = Arc::new(ThreadWake {
        thread: thread::current(),
        wakes: AtomicUsize::new(0),
    });
    let waker = TaskWaker::from(Arc::clone(&wake));
    let mut cx = Context::from_waker(&waker);
    let mut future = pin!(future);
    let until = Instant::now() + GUARD;
    let mut pending = false;
    loop {
        if let task::Poll::Ready(output) = future.as_mut().poll(&mut cx) {
            return (output, pending, wake.wakes.load(Ordering::SeqCst));
        }
        pending = true;
        assert!(Instant::now() < until, "the future resolves");
        thread::park_timeout(GUARD);
    }
}

#[test]
fn the_async_adapter_resolves_operations_and_blocks_by_waking_its_task() {
    let (_device, board) = rx_streaming(&[]);
    let (mut fake, _, dropped) = Fake::new();
    fake.io.board = board;
    let adapter = AsyncUsb::start(|registry, token, waker| {
        NativeUsb::attach(fake, session(), registry, token, waker).map_err(Error::Open)
    })
    .unwrap();
    let (read, pending, wakes) = block_on(adapter.operation(NativeUsb::start_temperature));
    assert!(
        matches!(read, Ok(NativeEvent::Temperature(Ok(celsius))) if (celsius - 25.0).abs() < f32::EPSILON),
        "{read:?}"
    );
    assert!(
        pending && wakes > 0,
        "the read waited for the worker and woke its task"
    );
    let open = adapter.operation(|native, _| native.start_rx_open(RX_BLOCK));
    let (second, ..) = block_on(adapter.operation(NativeUsb::start_temperature));
    assert!(
        matches!(second, Err(Error::Busy)),
        "one operation at a time: {second:?}"
    );
    let (opened, ..) = block_on(open);
    assert!(
        matches!(opened, Ok(NativeEvent::RxOpened(Ok(())))),
        "{opened:?}"
    );
    for _ in 0..3 {
        let (block, ..) = block_on(adapter.next_block());
        assert!(
            matches!(&block, Some(Ok(block)) if block.payload.len() == 4096),
            "{block:?}"
        );
    }
    let (refused, ..) =
        block_on(adapter.operation(|native, now| native.start_sample_rate(20_000_000, now)));
    assert!(
        matches!(refused, Err(Error::NotPowered)),
        "a start's refusal resolves its future: {refused:?}"
    );
    let (closed, ..) = block_on(adapter.close());
    closed.unwrap();
    assert_ne!(
        dropped.recv_timeout(GUARD).unwrap(),
        thread::current().id(),
        "the worker released the device"
    );
}

#[test]
fn an_async_session_end_fails_the_running_operation_and_ends_the_blocks() {
    let (fake, record, _) = Fake::new();
    let adapter = AsyncUsb::start(|registry, token, waker| {
        NativeUsb::attach(fake, session(), registry, token, waker).map_err(Error::Open)
    })
    .unwrap();
    record.lock().unwrap().panic_turn = true;
    let (read, ..) = block_on(adapter.operation(NativeUsb::start_temperature));
    assert!(
        matches!(read, Ok(NativeEvent::Temperature(Err(_)))),
        "the operation's event carries the worker's failure: {read:?}"
    );
    let (block, ..) = block_on(adapter.next_block());
    assert!(block.is_none(), "no block follows the session's end");
    let (after, ..) = block_on(adapter.operation(NativeUsb::start_temperature));
    assert!(after.is_err(), "the ended session refuses: {after:?}");
    let (closed, ..) = block_on(adapter.close());
    closed.unwrap();
}
