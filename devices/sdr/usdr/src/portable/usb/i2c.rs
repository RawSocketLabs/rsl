//! Bounded uSDR USB I²C transactions, advanced only by elapsed time and transfer events.

use core::marker::PhantomData;
use core::time::Duration;

use super::super::address::{I2C_MAX_READ, I2C_MAX_WRITE, I2cAddr};
use super::super::error::Failure;
use super::protocol::{Command, I2cCache, Notification};
pub(crate) use super::transfer::Event;

/// `USB_IO_TIMEOUT` in the pinned C transport, before the separate IRQ wait.
const WRITE_TIMEOUT: Duration = Duration::from_secs(20);
/// `usb_read_bus` allows one second for the command's IRQ completion.
const IRQ_TIMEOUT: Duration = Duration::from_secs(1);

/// A whole transfer the executor must perform, or the transaction's terminal result.
#[derive(Debug)]
pub(crate) enum Action {
    /// Submit one register command, retaining its identity until write completion.
    Write {
        id: u64,
        packet: [u8; 12],
        timeout: Duration,
    },
    /// Receive one complete IRQ packet without restarting the completion deadline.
    Notification { id: u64, timeout: Duration },
    /// Readback bytes in I²C wire order; only the first `len` bytes are valid.
    Complete { bytes: [u8; 4], len: usize },
}

/// Only one command and one scalar completion can be in flight for this transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// The packet is prepared, but no executor owns a transfer yet.
    Queued,
    /// The command has been handed to the executor.
    Writing,
    /// Waiting for the scalar I²C IRQ, possibly among unrelated notifications.
    Notification,
    /// The terminal result was delivered exactly once.
    Complete,
    /// Cancellation, invalid events or an ambiguous transport failure forbid reuse.
    Failed,
}

/// One private I²C operation; session cache and notification sequence remain externally owned.
pub(crate) struct I2cTransaction<E> {
    /// Executor correlation identity, never derived from the FPGA IRQ number.
    id: u64,
    /// Command header plus the address table and core control word.
    packet: [u8; 12],
    /// Number of requested readback bytes.
    read_len: usize,
    /// The next valid executor transition.
    phase: Phase,
    /// Most recent caller-supplied elapsed time.
    last: Duration,
    /// Deadline for the current phase.
    deadline: Duration,
    /// Errors arrive from the executor and are never stored or given an OS trait bound.
    error: PhantomData<fn() -> E>,
}

impl<E> I2cTransaction<E> {
    /// Prepares the bounded LUT/control packet without submitting transport work.
    pub(crate) fn new(
        id: u64,
        dev: I2cAddr,
        write: &[u8],
        read_len: usize,
        cache: &mut I2cCache,
        now: Duration,
    ) -> Result<Self, Failure<E>> {
        if dev.core() != 0
            || dev.bus() > 1
            || write.len() > I2C_MAX_WRITE
            || read_len > I2C_MAX_READ
        {
            return Err(Failure::Unsupported(
                "I2C transfer over the uSDR core limits",
            ));
        }
        let deadline = now
            .checked_add(WRITE_TIMEOUT)
            .ok_or_else(|| invalid("USB I2C time overflow"))?;
        let command = cache.command(
            dev.addr(),
            dev.bus(),
            write,
            u8::try_from(read_len).expect("invariant: four input bytes"),
        );
        let mut packet = [0; 12];
        packet[..4].copy_from_slice(&Command::encode(0, 2, false));
        packet[4..8].copy_from_slice(&command[0].to_le_bytes());
        packet[8..].copy_from_slice(&command[1].to_le_bytes());
        Ok(Self {
            id,
            packet,
            read_len,
            phase: Phase::Queued,
            last: now,
            deadline,
            error: PhantomData,
        })
    }

    /// Hands the command to the executor once; subsequent transitions require events.
    pub(crate) fn start(&mut self, now: Duration) -> Result<Action, Failure<E>> {
        if self.phase != Phase::Queued {
            return self.fail(invalid("USB I2C command was already started or terminated"));
        }
        let timeout = self.remaining(now)?;
        self.phase = Phase::Writing;
        Ok(Action::Write {
            id: self.id,
            packet: self.packet,
            timeout,
        })
    }

    /// Advances a timer without an I/O callback; terminal operations request no deadline.
    ///
    /// Expiration produces one failure. The executor still owns any outstanding transfer
    /// and must invalidate/clean up its session independently of observer completion.
    pub(crate) fn advance(&mut self, now: Duration) -> Result<Option<Duration>, Failure<E>> {
        if matches!(self.phase, Phase::Complete | Phase::Failed) {
            return Ok(None);
        }
        self.remaining(now).map(Some)
    }

    /// Refreshes an action's timeout immediately before submission by any executor.
    ///
    /// Scheduling delay must not restart a phase's deadline. Terminal results perform no
    /// I/O and remain deliverable without a remaining timer budget.
    pub(crate) fn dispatch(&mut self, action: Action, now: Duration) -> Result<Action, Failure<E>> {
        let remaining = self.advance(now)?;
        match (action, remaining) {
            (
                Action::Write {
                    id,
                    packet,
                    timeout,
                },
                Some(remaining),
            ) if id == self.id && self.phase == Phase::Writing => Ok(Action::Write {
                id,
                packet,
                timeout: timeout.min(remaining),
            }),
            (Action::Notification { id, timeout }, Some(remaining))
                if id == self.id && self.phase == Phase::Notification =>
            {
                Ok(Action::Notification {
                    id,
                    timeout: timeout.min(remaining),
                })
            }
            (complete @ Action::Complete { .. }, None) if self.phase == Phase::Complete => {
                Ok(complete)
            }
            _ => self.fail(invalid("USB I2C action differs from the active transfer")),
        }
    }

    /// Advances one correlated result, preserving one deadline across unrelated IRQs.
    pub(crate) fn complete(
        &mut self,
        event: Event<'_, E>,
        now: Duration,
        next_sequence: &mut Option<u16>,
    ) -> Result<Action, Failure<E>> {
        if event.id() != self.id {
            return self.fail(invalid("USB I2C completion identity differs from request"));
        }
        if !matches!(self.phase, Phase::Writing | Phase::Notification) {
            return self.fail(invalid(
                "USB I2C completion arrived outside an active transfer",
            ));
        }
        let remaining = self.remaining(now)?;
        match event {
            Event::Failed { error, .. } => self.fail(Failure::Transport(error)),
            Event::Written { .. } if self.phase == Phase::Writing => {
                if self.read_len == 0 {
                    self.phase = Phase::Complete;
                    return Ok(Action::Complete {
                        bytes: [0; 4],
                        len: 0,
                    });
                }
                let Some(deadline) = now.checked_add(IRQ_TIMEOUT) else {
                    return self.fail(invalid("USB I2C time overflow"));
                };
                self.deadline = deadline;
                self.phase = Phase::Notification;
                Ok(Action::Notification {
                    id: self.id,
                    timeout: IRQ_TIMEOUT,
                })
            }
            Event::Notification { packet, .. } if self.phase == Phase::Notification => {
                match Notification::completion(packet, 6, next_sequence) {
                    Ok(Some(value)) => {
                        self.phase = Phase::Complete;
                        Ok(Action::Complete {
                            bytes: value.to_le_bytes(),
                            len: self.read_len,
                        })
                    }
                    Ok(None) => Ok(Action::Notification {
                        id: self.id,
                        timeout: remaining,
                    }),
                    Err(reason) => self.fail(invalid(reason)),
                }
            }
            _ => self.fail(invalid("USB I2C completion kind differs from transfer")),
        }
    }

    /// Cancels queued work locally; issued work requires executor cleanup and session invalidation.
    #[cfg(test)]
    pub(crate) fn cancel(&mut self, now: Duration) -> Result<(), Failure<E>> {
        self.remaining(now)?;
        let queued = self.phase == Phase::Queued;
        self.phase = Phase::Failed;
        if queued {
            Ok(())
        } else {
            Err(Failure::Unsupported(
                "issued USB I2C cancellation requires reopening the session",
            ))
        }
    }

    /// Checks monotonic elapsed time and the current phase deadline before any transition.
    fn remaining(&mut self, now: Duration) -> Result<Duration, Failure<E>> {
        if now < self.last {
            return self.fail(invalid("USB I2C time moved backwards"));
        }
        self.last = now;
        match self.deadline.checked_sub(now) {
            Some(remaining) if !remaining.is_zero() => Ok(remaining),
            _ => self.fail(Failure::Timeout),
        }
    }

    /// A terminal failure never forgets that transport cleanup may remain outstanding.
    fn fail<T>(&mut self, error: Failure<E>) -> Result<T, Failure<E>> {
        self.phase = Phase::Failed;
        Err(error)
    }
}

/// A malformed event or clock violates the transaction contract.
fn invalid<E>(reason: &'static str) -> Failure<E> {
    Failure::InvalidData(reason)
}
