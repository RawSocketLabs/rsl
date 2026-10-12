//! Queue ownership and completed-transfer copying, exclusively on the native worker.

use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use mio::event::Source;
use rsl_usb::{InQueue, Interface, OutQueue, ReapProgress};

use super::super::i2c::{Action, Event, I2cTransaction};
use crate::lowlevel::BusError;

/// One bounded endpoint action with an absolute, non-renewable submission deadline.
pub(in super::super) struct Request {
    /// Session operation identity, also retained on its completion.
    pub id: u64,
    /// Last instant at which submission is still permitted.
    pub expires: Instant,
    /// The single endpoint transfer to submit.
    pub transfer: Transfer,
}

/// The pilot's fixed command or whole IRQ read, never a terminal machine action.
pub(in super::super) enum Transfer {
    /// The command produced by the real portable I²C machine.
    Write([u8; 12]),
    /// One whole notification transfer on endpoint 0x82.
    Notification,
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
    /// Complete IRQ packet copied after reap; malformed packets reach the real parser.
    Notification(Vec<u8>),
}

impl Completion {
    /// Delivers this owned result through the production machine without copying its logic.
    pub(in super::super) fn apply(
        self,
        transaction: &mut I2cTransaction,
        now: Duration,
        sequence: &mut Option<u16>,
    ) -> Result<Action, BusError> {
        let event = match self.result {
            Ok(Reply::Written) => Event::Written { id: self.id },
            Ok(Reply::Notification(ref packet)) => Event::Notification {
                id: self.id,
                packet,
            },
            Err(error) => Event::Failed { id: self.id, error },
        };
        transaction.complete(event, now, sequence)
    }
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

/// Real command and notification queues with one in-flight pilot request.
pub(in super::super) struct Queues {
    /// Drives bounded reaping; outlives the reactor's readiness, so the claim is released here.
    interface: Interface,
    /// Endpoint 0x01; never shared with the blocking control-plane pump.
    commands: OutQueue,
    /// Endpoint 0x82; never shared with another notification consumer.
    notifications: InQueue,
    /// Receive capacity larger than every valid 64-byte notification frame.
    notification_capacity: usize,
    /// Correlation and endpoint selection for the single submitted request.
    pending: Option<(u64, bool)>,
}

impl Queues {
    /// Takes an already-configured interface during blocking setup, before attachment.
    /// The device must be exclusive to this pilot: its readiness makes the reactor the only
    /// completion driver, and the adapter turns only while its own request is pending.
    #[expect(dead_code, reason = "no native driver handle opens real queues yet")]
    pub(in super::super) fn new(interface: Interface) -> Result<Self, BusError> {
        let commands = interface.out_queue(0x01).map_err(BusError::from)?;
        let notifications = interface.in_queue(0x82).map_err(BusError::from)?;
        let notification_capacity = Self::notification_capacity(notifications.max_packet_size())?;
        Ok(Self {
            interface,
            commands,
            notifications,
            notification_capacity,
            pending: None,
        })
    }

    /// Requires short-packet framing; an aligned valid prefix must not conceal a malformed
    /// IRQ tail. Aligned/full-speed layouts require separate device-side terminator evidence.
    pub(in super::super) fn notification_capacity(packet_size: usize) -> Result<usize, BusError> {
        if packet_size <= 64 {
            return Err(BusError::Unsupported(
                "native USB notifications require packet size over 64 bytes",
            ));
        }
        Ok(packet_size)
    }
}

impl Backend for Queues {
    fn readiness(&self) -> Result<Box<dyn Source + Send>, BusError> {
        Ok(Box::new(
            self.interface.readiness().map_err(BusError::from)?,
        ))
    }

    fn submit(&mut self, request: Request) -> Result<(), BusError> {
        if self.pending.is_some() {
            return Err(BusError::Unsupported("native USB request already pending"));
        }
        let write = matches!(request.transfer, Transfer::Write(_));
        match request.transfer {
            Transfer::Write(packet) => {
                let mut buffer = self.commands.alloc(packet.len());
                buffer.as_mut_slice().copy_from_slice(&packet);
                buffer.set_len(packet.len());
                if Instant::now() >= request.expires {
                    return Err(BusError::Timeout);
                }
                self.commands
                    .submit(buffer)
                    .map_err(|rejected| BusError::from(rejected.error))?;
            }
            Transfer::Notification => {
                let buffer = self
                    .notifications
                    .alloc(self.notification_capacity)
                    .map_err(BusError::from)?;
                if Instant::now() >= request.expires {
                    return Err(BusError::Timeout);
                }
                self.notifications
                    .submit(buffer)
                    .map_err(|rejected| BusError::from(rejected.error))?;
            }
        }
        self.pending = Some((request.id, write));
        Ok(())
    }

    fn turn(&mut self, budget: NonZeroUsize) -> Result<Turn, BusError> {
        // Busy, like an exhausted budget, calls for another turn rather than a readiness edge.
        let reaped = match self.interface.reap(budget) {
            Err(rsl_usb::Error::Busy) => return Ok(Turn::AGAIN),
            reaped => reaped,
        };
        // A completion filed before a reap error, even as its prefix, is still delivered.
        let completion = if let Some((id, write)) = self.pending {
            let done = match if write {
                self.commands.try_complete()
            } else {
                self.notifications.try_complete()
            } {
                Err(rsl_usb::Error::Busy) => return Ok(Turn::AGAIN),
                done => done.map_err(BusError::from)?,
            };
            done.map(|done| {
                self.pending = None;
                let result = done
                    .status
                    .map_err(rsl_usb::Error::from)
                    .map_err(BusError::from)
                    .map(|()| {
                        if write {
                            Reply::Written
                        } else {
                            Reply::Notification(done.buffer.to_vec())
                        }
                    });
                Completion { id, result }
            })
        } else {
            None
        };
        let exhausted = match reaped {
            Ok(progress) => progress == ReapProgress::BudgetExhausted,
            Err(error) if completion.is_none() => return Err(error.into()),
            // The error recurs on the next turn.
            Err(_) => true,
        };
        Ok(Turn {
            completion,
            exhausted,
        })
    }

    fn cleanup(&mut self) -> Result<(), BusError> {
        let commands = self.commands.cancel_all().map_err(BusError::from);
        let notifications = self.notifications.cancel_all().map_err(BusError::from);
        // Attempt both queues even when the first failed. Drop retains any remaining URBs.
        commands.and(notifications).map(|_| ())
    }
}
