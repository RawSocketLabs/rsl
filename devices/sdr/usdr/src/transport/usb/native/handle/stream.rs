//! The RX and TX streams on [`NativeUsb`], at the transport: opening, closing, RX blocks and
//! sends, over the blocking bus's own `Rx`/`Tx` adapters and in its send order (`TxDevice`'s
//! acquire, throttled status read, commit), with every USB call on the worker.

use std::time::{Duration, Instant};

use super::super::super::bus::tx_status_failure;
use super::super::super::machine::{Machine, Output};
use super::super::super::tx::STATUS_REGISTER;
use super::super::adapter::StreamOutcome;
use super::super::worker::StreamCommand;
use super::{NativeEvent, NativeUsb, Pending, StreamState};
use crate::error::Error;
use crate::lowlevel::BusError;
use crate::portable::usb::register::RegisterTransaction;

/// A stream's open or close, awaiting the worker's answer.
#[derive(Clone, Copy, Debug)]
pub(super) enum StreamOperation {
    /// RX opens and primes its ring.
    RxOpen,
    /// RX cancels and releases its endpoint.
    RxClose,
    /// TX opens.
    TxOpen,
    /// TX cancels and releases its endpoint.
    TxClose,
}

/// A send in flight, by phase: `TxDevice::send`'s order.
#[expect(
    clippy::large_enum_variant,
    reason = "one per handle; boxing the machine would allocate per send"
)]
pub(super) enum Send {
    /// Waiting for a free transfer until `until`; `tried` once an attempt found every
    /// transfer in flight, so the deadline applies.
    Acquire {
        time: Option<u64>,
        block: Vec<u8>,
        until: Instant,
        tried: bool,
    },
    /// Reading the TX core's status words over the control plane.
    Status {
        time: Option<u64>,
        block: Vec<u8>,
        machine: Machine,
    },
    /// The worker frames and queues the block, which reports these status words.
    Commit([u32; 4]),
}

impl Send {
    /// When the send times out, once an attempt has found no free transfer.
    pub(super) fn deadline(&self) -> Option<Instant> {
        match *self {
            Self::Acquire {
                until, tried: true, ..
            } => Some(until),
            _ => None,
        }
    }
}

impl NativeUsb {
    /// Starts opening RX endpoint 0x83 for blocks of `block_bytes` sample bytes (each
    /// followed on the wire by the transport's 16-byte trailer) and priming its eight
    /// transfers; [`poll`](Self::poll) reports [`NativeEvent::RxOpened`], then each block
    /// as [`NativeEvent::Received`]. It configures no stream engine: the gateware must
    /// already be producing blocks of that size, as after
    /// [`start_create_streams`](Self::start_create_streams), which opens RX itself.
    ///
    /// Blocks are copied off their transfers on the worker, which requeues each at once;
    /// at most eight copies wait for [`poll`](Self::poll), after which the device's ring
    /// runs dry and the gateware counts the blocks it drops (`RxBlock::oob`).
    ///
    /// # Errors
    ///
    /// [`Error::Busy`] if an operation is pending; [`Error::AlreadyStreaming`] unless RX
    /// is closed, or while a stream group exists; [`Error::Stream`] if the handle is closed.
    pub fn start_rx_open(&mut self, block_bytes: u32) -> Result<(), Error> {
        if self.pending.is_some() {
            return Err(Error::Busy);
        }
        self.refuse_in_group()?;
        if self.rx != StreamState::Closed {
            return Err(Error::AlreadyStreaming);
        }
        self.start_stream(StreamOperation::RxOpen, StreamCommand::RxOpen(block_bytes))
    }

    /// Starts closing RX: blocks not yet polled are dropped, and the worker cancels every
    /// transfer and releases the endpoint once all are back, which may take it seconds;
    /// [`poll`](Self::poll) reports [`NativeEvent::RxClosed`]. Closing a closed stream
    /// succeeds.
    ///
    /// # Errors
    ///
    /// [`Error::Busy`] if an operation is pending; [`Error::AlreadyStreaming`] while a
    /// stream group exists (stop it instead); [`Error::Stream`] if the handle is closed.
    pub fn start_rx_close(&mut self) -> Result<(), Error> {
        if self.pending.is_some() {
            return Err(Error::Busy);
        }
        self.refuse_in_group()?;
        self.start_stream(StreamOperation::RxClose, StreamCommand::RxClose)
    }

    /// Starts opening TX endpoint 0x03 for blocks of up to `block_bytes` of samples of
    /// `sample_bytes` (64 to 126976 bytes, as libusdr); [`poll`](Self::poll) reports
    /// [`NativeEvent::TxOpened`]. Transfers are allocated as sends need them, up to 32 in
    /// flight. It configures no stream engine;
    /// [`start_create_streams`](Self::start_create_streams) opens TX itself.
    ///
    /// # Errors
    ///
    /// [`Error::Busy`] if an operation is pending; [`Error::AlreadyStreaming`] unless TX
    /// is closed, or while a stream group exists; [`Error::Stream`] if the handle is closed.
    pub fn start_tx_open(&mut self, block_bytes: u32, sample_bytes: u32) -> Result<(), Error> {
        if self.pending.is_some() {
            return Err(Error::Busy);
        }
        self.refuse_in_group()?;
        if self.tx != StreamState::Closed {
            return Err(Error::AlreadyStreaming);
        }
        let command = StreamCommand::TxOpen {
            block_bytes,
            sample_bytes,
        };
        self.start_stream(StreamOperation::TxOpen, command)
    }

    /// Starts sending `block`, wire-format samples, to play at `time` in samples (`None`:
    /// as soon as it arrives), as the blocking bus sends: a free transfer, waiting up to
    /// `timeout` for one while all 32 are in flight; then the TX core's status words, read
    /// over the control plane on the device's first counted send and every 64th after;
    /// then the block behind libusdr's 16-byte header. [`poll`](Self::poll) reports
    /// [`NativeEvent::Sent`]. A block of no bytes, more than the stream's, or part samples
    /// is refused there, after its status read, as on the blocking bus.
    ///
    /// # Errors
    ///
    /// [`Error::Busy`] if an operation is pending; [`Error::NotStreaming`] unless TX is
    /// open; [`Error::Stream`] if the handle is closed or `timeout` overflows the clock.
    pub fn start_send(
        &mut self,
        time: Option<u64>,
        block: &[u8],
        timeout: Duration,
        now: Instant,
    ) -> Result<(), Error> {
        if self.pending.is_some() {
            return Err(Error::Busy);
        }
        if self.tx != StreamState::Open {
            return Err(Error::NotStreaming);
        }
        if self.adapter.is_stopped() {
            return Err(Error::Stream(BusError::Disconnected));
        }
        let until = now
            .checked_add(timeout)
            .ok_or(Error::Stream(BusError::Unsupported(
                "native send timeout overflows the clock",
            )))?;
        self.pending = Some(Pending::Send(Send::Acquire {
            time,
            block: block.to_vec(),
            until,
            tried: false,
        }));
        self.adapter.want_tx(true);
        Ok(())
    }

    /// Starts closing TX: the worker cancels every transfer, sent or not (libusdr's
    /// `buffers_usb_free`), and releases the endpoint once all are back, which may take it
    /// seconds; [`poll`](Self::poll) reports [`NativeEvent::TxClosed`]. Closing a closed
    /// stream succeeds.
    ///
    /// # Errors
    ///
    /// [`Error::Busy`] if an operation is pending; [`Error::AlreadyStreaming`] while a
    /// stream group exists (stop it instead); [`Error::Stream`] if the handle is closed.
    pub fn start_tx_close(&mut self) -> Result<(), Error> {
        if self.pending.is_some() {
            return Err(Error::Busy);
        }
        self.refuse_in_group()?;
        self.start_stream(StreamOperation::TxClose, StreamCommand::TxClose)
    }

    /// Hands a stream command to the worker as the operation in flight.
    fn start_stream(
        &mut self,
        operation: StreamOperation,
        command: StreamCommand,
    ) -> Result<(), Error> {
        self.begin_stream(operation, command)?;
        self.pending = Some(Pending::Stream(operation));
        Ok(())
    }

    /// Ends a stream command once the worker answers it.
    pub(super) fn poll_stream(&mut self, operation: StreamOperation) -> Option<NativeEvent> {
        let Some(result) = self.command_result() else {
            self.pending = Some(Pending::Stream(operation));
            return None;
        };
        let result = self.settle_stream(operation, result);
        Some(match operation {
            StreamOperation::RxOpen => NativeEvent::RxOpened(result),
            StreamOperation::RxClose => NativeEvent::RxClosed(result),
            StreamOperation::TxOpen => NativeEvent::TxOpened(result),
            StreamOperation::TxClose => NativeEvent::TxClosed(result),
        })
    }

    /// Hands `command` to the worker for `operation`; a close takes its stream out of
    /// service at once.
    pub(super) fn begin_stream(
        &mut self,
        operation: StreamOperation,
        command: StreamCommand,
    ) -> Result<(), Error> {
        self.adapter.command(command).map_err(Error::Stream)?;
        match operation {
            StreamOperation::RxClose => {
                self.rx = StreamState::Closed;
                self.adapter.want_rx(false);
            }
            StreamOperation::TxClose => self.tx = StreamState::Closed,
            StreamOperation::RxOpen | StreamOperation::TxOpen => {}
        }
        Ok(())
    }

    /// Records the worker's answer to a stream command in the stream's state.
    pub(super) fn settle_stream(
        &mut self,
        operation: StreamOperation,
        result: Result<(), BusError>,
    ) -> Result<(), Error> {
        let result = result.map_err(Error::Stream);
        let (opened, closed) = if result.is_ok() {
            (StreamState::Open, StreamState::Closed)
        } else {
            (StreamState::Failed, StreamState::Failed)
        };
        match operation {
            StreamOperation::RxOpen => {
                self.rx = opened;
                if result.is_ok() {
                    self.adapter.want_rx(true);
                }
            }
            StreamOperation::RxClose => self.rx = closed,
            StreamOperation::TxOpen => self.tx = opened,
            StreamOperation::TxClose => self.tx = closed,
        }
        result
    }

    /// Refuses a transport-level open or close while a stream group holds the streams:
    /// the group's own operations open and close them.
    fn refuse_in_group(&self) -> Result<(), Error> {
        if self.group.is_some() {
            return Err(Error::AlreadyStreaming);
        }
        Ok(())
    }

    /// The worker's answer to the outstanding stream command, or the failure that ended
    /// the session; `None` while it is outstanding. A late acquisition for a send that
    /// timed out is passed over: its transfer stays held for the next send.
    pub(super) fn command_result(&mut self) -> Option<Result<(), BusError>> {
        loop {
            match self.adapter.try_outcome() {
                Some(StreamOutcome::Command(result)) => return Some(result),
                Some(StreamOutcome::Acquired(_)) => {}
                None if self.adapter.is_stopped() => return Some(Err(self.fault())),
                None => return None,
            }
        }
    }

    /// Advances a send through its phases.
    pub(super) fn poll_send(&mut self, send: Send, now: Instant) -> Option<NativeEvent> {
        match send {
            Send::Acquire {
                time,
                block,
                until,
                mut tried,
            } => {
                while let Some(outcome) = self.adapter.try_outcome() {
                    let StreamOutcome::Acquired(acquired) = outcome else {
                        continue;
                    };
                    match acquired {
                        Ok(true) => {
                            self.adapter.want_tx(false);
                            return self.acquired(time, block, now);
                        }
                        Ok(false) => tried = true,
                        Err(error) => return Some(self.unsent(Error::Stream(error))),
                    }
                }
                if self.adapter.is_stopped() {
                    let fault = self.fault();
                    return Some(self.unsent(Error::Stream(fault)));
                }
                if tried && now >= until {
                    return Some(self.unsent(Error::Timeout));
                }
                self.pending = Some(Pending::Send(Send::Acquire {
                    time,
                    block,
                    until,
                    tried,
                }));
                None
            }
            Send::Status {
                time,
                block,
                mut machine,
            } => {
                let Some(outcome) = self.observe(&mut machine, now) else {
                    self.pending = Some(Pending::Send(Send::Status {
                        time,
                        block,
                        machine,
                    }));
                    return None;
                };
                let words = match outcome {
                    Ok(Some(Output::Words { words, len: 4 })) => {
                        [words[0], words[1], words[2], words[3]]
                    }
                    Ok(_) => {
                        return Some(self.status_failed(BusError::Unsupported(
                            "USB TX status read completed without four words",
                        )));
                    }
                    Err(error) => return Some(self.status_failed(error)),
                };
                self.session.tx_status.record(words);
                self.commit(time, block, words)
            }
            Send::Commit(words) => {
                let Some(result) = self.command_result() else {
                    self.pending = Some(Pending::Send(Send::Commit(words)));
                    return None;
                };
                Some(NativeEvent::Sent(
                    result.map(|()| words).map_err(Error::Stream),
                ))
            }
        }
    }

    /// A free transfer is held: counts the send, reading the status words afresh on every
    /// 64th, then commits.
    fn acquired(&mut self, time: Option<u64>, block: Vec<u8>, now: Instant) -> Option<NativeEvent> {
        if !self.session.tx_status.count_send() {
            let words = self.session.tx_status.words();
            return self.commit(time, block, words);
        }
        let address = u16::try_from(STATUS_REGISTER)
            .expect("invariant: the TX status words are physical registers");
        let elapsed = self.elapsed(now);
        let read = self
            .take_id()
            .and_then(|id| Ok(RegisterTransaction::read(id, address, 4, elapsed)?))
            .and_then(|read| self.launch(Machine::Register(read), now));
        match read {
            Ok(machine) => {
                self.pending = Some(Pending::Send(Send::Status {
                    time,
                    block,
                    machine,
                }));
                None
            }
            Err(error) => Some(self.status_failed(error)),
        }
    }

    /// Hands the block, its time and status words to the worker to frame and queue.
    fn commit(
        &mut self,
        time: Option<u64>,
        block: Vec<u8>,
        words: [u32; 4],
    ) -> Option<NativeEvent> {
        let command = StreamCommand::TxCommit {
            time,
            status: words,
            block,
        };
        match self.adapter.command(command) {
            Ok(()) => {
                self.pending = Some(Pending::Send(Send::Commit(words)));
                None
            }
            Err(error) => Some(NativeEvent::Sent(Err(Error::Stream(error)))),
        }
    }

    /// A send that ends before its commit; it stops wanting a free transfer.
    fn unsent(&mut self, error: Error) -> NativeEvent {
        self.adapter.want_tx(false);
        NativeEvent::Sent(Err(error))
    }

    /// A failed status read: a control-plane failure, so it closes the handle. A timeout is
    /// reported as I/O, as the blocking bus reports it, never as a full ring.
    fn status_failed(&mut self, error: BusError) -> NativeEvent {
        self.adapter.retire_after_completion();
        NativeEvent::Sent(Err(Error::Stream(tx_status_failure(error))))
    }

    /// The next RX block, or the end of a failed RX stream.
    pub(super) fn poll_rx(&mut self) -> Option<NativeEvent> {
        if self.rx != StreamState::Open {
            return None;
        }
        let error = match self.adapter.try_block() {
            Some(Ok(block)) => return Some(NativeEvent::Received(Ok(block))),
            Some(Err(error)) => error,
            None if self.adapter.is_stopped() => self.fault(),
            None => return None,
        };
        self.rx = StreamState::Failed;
        self.adapter.want_rx(false);
        Some(NativeEvent::Received(Err(Error::Stream(error))))
    }

    /// Why the session ended, for a stream that outlived it.
    fn fault(&mut self) -> BusError {
        self.adapter.take_fault().unwrap_or(BusError::Disconnected)
    }
}
