//! Queue ownership and completed-transfer copying, exclusively on the native worker.

use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use mio::event::Source;
use rsl_usb::{InQueue, Interface, OutQueue, ReapProgress};

use super::super::bus::{MAX_NOTIFICATION_BYTES, transfer_length};
use super::super::i2c::Event;
use super::super::machine::{Machine, Step, Transfer};
use crate::lowlevel::{BusError, usb_error};
use crate::portable::usb::register::MAX_READ_WORDS;

/// One bounded endpoint action with an absolute, non-renewable submission deadline.
pub(in super::super) struct Request {
    /// Session operation identity, also retained on its completion.
    pub id: u64,
    /// Last instant at which submission is still permitted.
    pub expires: Instant,
    /// The single endpoint transfer to submit.
    pub transfer: Transfer,
}

/// A worker-owned result containing no kernel-owned memory or borrowed queue data.
pub(in super::super) struct Completion {
    /// Correlates with exactly one issued request.
    pub id: u64,
    /// Native failures are preserved, not stringified.
    pub result: Result<Reply, BusError>,
}

/// Successfully returned bytes or the command's completed write.
pub(in super::super) enum Reply {
    /// The kernel completed the command transfer, not just its submission.
    Written,
    /// Complete register readback copied after reap; malformed lengths reach the machine.
    Readback(Vec<u8>),
    /// Complete IRQ packet copied after reap; malformed packets reach the real parser.
    Notification(Vec<u8>),
}

impl Completion {
    /// Delivers this owned result through the production machine without copying its logic.
    pub(in super::super) fn apply(
        self,
        machine: &mut Machine,
        now: Duration,
        sequence: &mut Option<u16>,
    ) -> Result<Step, BusError> {
        let event = match self.result {
            Ok(Reply::Written) => Event::Written { id: self.id },
            Ok(Reply::Readback(ref packet)) => Event::Readback {
                id: self.id,
                packet,
            },
            Ok(Reply::Notification(ref packet)) => Event::Notification {
                id: self.id,
                packet,
            },
            Err(error) => Event::Failed { id: self.id, error },
        };
        machine.complete(event, now, sequence)
    }
}

/// The endpoint a submitted request completes on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Endpoint {
    /// Command endpoint 0x01.
    Commands,
    /// Register readback endpoint 0x81.
    Registers,
    /// Notification endpoint 0x82.
    Notifications,
}

/// One bounded worker reap turn; exhausting its budget demands another turn, not an edge.
pub(in super::super) struct Turn {
    /// At most one endpoint request can complete in this pilot.
    pub completion: Option<Completion>,
    /// The backend may still contain completions without another readiness edge.
    pub exhausted: bool,
}

impl Turn {
    /// A turn that completed nothing and must be repeated before waiting for readiness.
    const AGAIN: Self = Self {
        completion: None,
        exhausted: true,
    };
}

/// Exclusive worker boundary, implemented by real queues and the whole-transfer test fake.
/// Serving errors and panics must not release or replace its owner.
pub(in super::super) trait Backend: Send + 'static {
    /// The device's readiness, taken once during blocking setup. The adapter drops it before
    /// retiring the worker, so it may refer to a descriptor this backend keeps open, and the
    /// backend's Drop, not the reactor's, releases the device.
    fn readiness(&self) -> Result<Box<dyn Source + Send>, BusError>;
    /// Submits only; never waits for transfer completion. Recheck expiry after allocation
    /// and immediately before touching the kernel.
    fn submit(&mut self, request: Request) -> Result<(), BusError>;
    /// Reaps within the supplied budget and takes only already-filed completions.
    fn turn(&mut self, budget: NonZeroUsize) -> Result<Turn, BusError>;
    /// Cancels/drains owned transfers; this and subsequent Drop may block.
    fn cleanup(&mut self) -> Result<(), BusError>;
}

/// Real command, readback and notification queues with one in-flight request.
pub(in super::super) struct Queues {
    /// Drives bounded reaping; outlives the reactor's readiness, so the claim is released here.
    interface: Interface,
    /// Endpoint 0x01; never shared with the blocking control-plane pump.
    commands: OutQueue,
    /// Endpoint 0x81; never shared with another readback consumer.
    registers: InQueue,
    /// Endpoint 0x82; never shared with another notification consumer.
    notifications: InQueue,
    /// Receive capacity: whole packets holding the largest (256-byte) readback.
    readback_capacity: usize,
    /// Receive capacity: whole packets holding the largest (64-byte) notification.
    notification_capacity: usize,
    /// Correlation and endpoint selection for the single submitted request.
    pending: Option<(u64, Endpoint)>,
}

impl Queues {
    /// Takes an already-configured interface during blocking setup, before attachment.
    /// The device must be exclusive to this pilot: its readiness makes the reactor the only
    /// completion driver, and the adapter turns only while its own request is pending.
    pub(in super::super) fn new(interface: Interface) -> Result<Self, BusError> {
        let commands = interface.out_queue(0x01).map_err(usb_error)?;
        let registers = interface.in_queue(0x81).map_err(usb_error)?;
        let notifications = interface.in_queue(0x82).map_err(usb_error)?;
        let readback_capacity = Self::readback_capacity(registers.max_packet_size())?;
        let notification_capacity = Self::notification_capacity(notifications.max_packet_size())?;
        Ok(Self {
            interface,
            commands,
            registers,
            notifications,
            readback_capacity,
            notification_capacity,
            pending: None,
        })
    }

    /// One readback transfer's length, as the blocking bus sizes it. Like `UsbBus::open`,
    /// refuses packets of 256 bytes or fewer: a packet-aligned readback would then wait for
    /// a zero-length terminator the device does not send.
    pub(in super::super) fn readback_capacity(packet_size: usize) -> Result<usize, BusError> {
        let largest = MAX_READ_WORDS * 4;
        if packet_size <= largest {
            return Err(BusError::Unsupported(
                "USB readback requires a high-speed endpoint (packet size over 256 bytes)",
            ));
        }
        transfer_length(largest, packet_size)
    }

    /// One notification transfer's length, sized exactly as the blocking bus and the pinned
    /// C transport size theirs. With packets of 64 bytes or fewer a valid 64-byte record can
    /// fill the transfer, so an overlong reply is not detected; neither path detects it.
    pub(in super::super) fn notification_capacity(packet_size: usize) -> Result<usize, BusError> {
        transfer_length(MAX_NOTIFICATION_BYTES, packet_size)
    }

    /// Submits one whole-reply receive of `capacity` bytes unless `expires` has passed.
    fn receive(queue: &mut InQueue, capacity: usize, expires: Instant) -> Result<(), BusError> {
        let buffer = queue.alloc(capacity).map_err(usb_error)?;
        if Instant::now() >= expires {
            return Err(BusError::Timeout);
        }
        queue
            .submit(buffer)
            .map_err(|rejected| usb_error(rejected.error))
    }
}

impl Backend for Queues {
    fn readiness(&self) -> Result<Box<dyn Source + Send>, BusError> {
        Ok(Box::new(self.interface.readiness().map_err(usb_error)?))
    }

    fn submit(&mut self, request: Request) -> Result<(), BusError> {
        if self.pending.is_some() {
            return Err(BusError::Unsupported("native USB request already pending"));
        }
        let endpoint = match request.transfer {
            Transfer::Write(frame) => {
                let bytes = frame.as_bytes();
                let mut buffer = self.commands.alloc(bytes.len());
                buffer.as_mut_slice().copy_from_slice(bytes);
                buffer.set_len(bytes.len());
                if Instant::now() >= request.expires {
                    return Err(BusError::Timeout);
                }
                self.commands
                    .submit(buffer)
                    .map_err(|rejected| usb_error(rejected.error))?;
                Endpoint::Commands
            }
            Transfer::Readback => {
                Self::receive(&mut self.registers, self.readback_capacity, request.expires)?;
                Endpoint::Registers
            }
            Transfer::Notification => {
                Self::receive(
                    &mut self.notifications,
                    self.notification_capacity,
                    request.expires,
                )?;
                Endpoint::Notifications
            }
        };
        self.pending = Some((request.id, endpoint));
        Ok(())
    }

    fn turn(&mut self, budget: NonZeroUsize) -> Result<Turn, BusError> {
        // Busy, like an exhausted budget, calls for another turn rather than a readiness edge.
        let reaped = match self.interface.reap(budget) {
            Err(rsl_usb::Error::Busy) => return Ok(Turn::AGAIN),
            reaped => reaped,
        };
        // A completion filed before a reap error, even as its prefix, is still delivered.
        let completion = if let Some((id, endpoint)) = self.pending {
            let done = match match endpoint {
                Endpoint::Commands => self.commands.try_complete(),
                Endpoint::Registers => self.registers.try_complete(),
                Endpoint::Notifications => self.notifications.try_complete(),
            } {
                Err(rsl_usb::Error::Busy) => return Ok(Turn::AGAIN),
                done => done.map_err(usb_error)?,
            };
            done.map(|done| {
                self.pending = None;
                let result = done
                    .status
                    .map_err(rsl_usb::Error::from)
                    .map_err(usb_error)
                    .map(|()| match endpoint {
                        Endpoint::Commands => Reply::Written,
                        Endpoint::Registers => Reply::Readback(done.buffer.to_vec()),
                        Endpoint::Notifications => Reply::Notification(done.buffer.to_vec()),
                    });
                Completion { id, result }
            })
        } else {
            None
        };
        let exhausted = match reaped {
            Ok(progress) => progress == ReapProgress::BudgetExhausted,
            Err(error) if completion.is_none() => return Err(usb_error(error)),
            // The error recurs on the next turn.
            Err(_) => true,
        };
        Ok(Turn {
            completion,
            exhausted,
        })
    }

    fn cleanup(&mut self) -> Result<(), BusError> {
        let commands = self.commands.cancel_all().map_err(usb_error);
        let registers = self.registers.cancel_all().map_err(usb_error);
        let notifications = self.notifications.cancel_all().map_err(usb_error);
        // Attempt every queue even when one failed. Drop retains any remaining URBs.
        commands.and(registers).and(notifications).map(|_| ())
    }
}
