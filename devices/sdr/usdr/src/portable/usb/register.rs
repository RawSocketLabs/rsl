//! Bounded uSDR USB register and SPI transactions, advanced only by elapsed time and
//! transfer events.
//!
//! Framing is `usb_async_regread32`/`usb_async_regwrite32` in the pinned C transport
//! (`lowlevel/usb_uram/usb_uram_libusb.c`): a read sends only the command word on endpoint
//! 0x01 and receives the raw words, without a length header, on 0x81; a write sends the
//! command word and its values on 0x01. An SPI exchange (`usb_uram_ls_op`) writes the word to
//! the SPI core's register 2, then waits for its scalar IRQ, event 2, on 0x82.

use core::marker::PhantomData;
use core::time::Duration;

use super::super::address::SpiAddr;
use super::super::error::Failure;
use super::super::sequence::FpgaAccess;
use super::protocol::{Command, Notification};
pub(crate) use super::transfer::Event;
use super::transfer::{Frame, MAX_FRAME_BYTES};

/// `MAX_REQUEST_RB_SIZE`: 256 bytes of raw readback.
pub(crate) const MAX_READ_WORDS: usize = 64;
/// `OUT_REGOUT_SIZE` includes the command word: 63 payload words fit in 256 bytes.
pub(crate) const MAX_WRITE_WORDS: usize = MAX_FRAME_BYTES / 4 - 1;
/// `USB_IO_TIMEOUT` in the pinned C transport, for the command and for its readback.
const IO_TIMEOUT: Duration = Duration::from_secs(20);
/// `usb_read_bus` allows one second for a command's IRQ completion.
const IRQ_TIMEOUT: Duration = Duration::from_secs(1);
/// The simple SPI core's data register.
pub(crate) const SPI_REGISTER: u16 = 2;
/// The SPI core's completion IRQ.
const SPI_EVENT: u8 = 2;
/// Physical registers the 16-bit command address reaches.
const REGISTER_SPACE: usize = 0x1_0000;

/// A whole transfer the executor must perform, or the transaction's terminal result.
#[derive(Debug)]
pub(crate) enum Action {
    /// Send one command frame on endpoint 0x01, keeping its identity until it completes.
    Write {
        id: u64,
        frame: Frame,
        timeout: Duration,
    },
    /// Receive one whole register readback transfer on endpoint 0x81.
    Readback { id: u64, timeout: Duration },
    /// Receive one whole IRQ transfer on 0x82 without restarting the completion deadline.
    Notification { id: u64, timeout: Duration },
    /// The words read, or the SPI reply as the single word; only the first `len` are valid.
    /// A register write completes with none.
    Complete {
        words: [u32; MAX_READ_WORDS],
        len: usize,
    },
}

/// What follows the command transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reply {
    /// Nothing: a register write.
    None,
    /// `count` raw words on 0x81.
    Readback { count: usize },
    /// The scalar IRQ `event` on 0x82, among any unrelated records.
    Notification { event: u8 },
}

/// Only one command and its reply can be in flight for this transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    /// The frame is prepared, but no executor owns a transfer yet.
    Queued,
    /// The command has been handed to the executor.
    Writing,
    /// Waiting for `count` raw readback words.
    Readback { count: usize },
    /// Waiting for the scalar IRQ `event`, possibly among unrelated notifications.
    Notification { event: u8 },
    /// The terminal result was delivered exactly once.
    Complete,
    /// Cancellation, invalid events or an ambiguous transport failure forbid reuse.
    Failed,
}

/// One private register read, register write or SPI exchange. The session's notification
/// sequence remains externally owned.
pub(crate) struct RegisterTransaction<E> {
    /// Executor correlation identity, never derived from the FPGA IRQ number.
    id: u64,
    /// The command transfer.
    frame: Frame,
    /// What the command's completion waits for next.
    reply: Reply,
    /// The next valid executor transition.
    phase: Phase,
    /// Most recent caller-supplied elapsed time.
    last: Duration,
    /// Deadline for the current phase.
    deadline: Duration,
    /// Errors arrive from the executor and are never stored or given an OS trait bound.
    error: PhantomData<fn() -> E>,
}

impl<E> RegisterTransaction<E> {
    /// Reads `count` (1–64) consecutive physical registers from `address`.
    pub(crate) fn read(
        id: u64,
        address: u16,
        count: usize,
        now: Duration,
    ) -> Result<Self, Failure<E>> {
        if !(1..=MAX_READ_WORDS).contains(&count) {
            return Err(Failure::Unsupported(
                "USB register reads carry 1 to 64 words",
            ));
        }
        Self::new(id, address, count, &[], Reply::Readback { count }, now)
    }

    /// Writes `values` (1–63 words) to consecutive physical registers from `address`.
    pub(crate) fn write(
        id: u64,
        address: u16,
        values: &[u32],
        now: Duration,
    ) -> Result<Self, Failure<E>> {
        if !(1..=MAX_WRITE_WORDS).contains(&values.len()) {
            return Err(Failure::Unsupported(
                "USB register writes carry 1 to 63 words",
            ));
        }
        Self::new(id, address, values.len(), values, Reply::None, now)
    }

    /// Shifts `word` through the uSDR's one SPI bus and waits for the word read back.
    pub(crate) fn spi32(
        id: u64,
        target: SpiAddr,
        word: u32,
        now: Duration,
    ) -> Result<Self, Failure<E>> {
        if target.0 != 0 {
            return Err(Failure::Unsupported("uSDR has only simple SPI bus 0"));
        }
        let reply = Reply::Notification { event: SPI_EVENT };
        Self::new(id, SPI_REGISTER, 1, &[word], reply, now)
    }

    /// The transaction performing one sequence action: a single physical-register read or
    /// write, or an SPI exchange. A register past the 16-bit command address (the stream
    /// engine's configuration space) is refused: its executor reaches it through the index
    /// window, one physical access at a time.
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "only the native executor (feature `mio`) runs sequences over USB"
        )
    )]
    pub(crate) fn access(id: u64, access: FpgaAccess, now: Duration) -> Result<Self, Failure<E>> {
        let physical = |reg: u32| {
            u16::try_from(reg).map_err(|_| {
                Failure::Unsupported("USB register commands address 16-bit physical registers")
            })
        };
        match access {
            FpgaAccess::Read { reg } => Self::read(id, physical(reg)?, 1, now),
            FpgaAccess::Write { reg, value } => Self::write(id, physical(reg)?, &[value], now),
            FpgaAccess::Spi { target, word, .. } => Self::spi32(id, target, word, now),
        }
    }

    /// Checks the span, frames the command for `count` words (sending `values`, if a
    /// write) and starts the command deadline.
    fn new(
        id: u64,
        address: u16,
        count: usize,
        values: &[u32],
        reply: Reply,
        now: Duration,
    ) -> Result<Self, Failure<E>> {
        if usize::from(address) + count > REGISTER_SPACE {
            return Err(Failure::Unsupported(
                "USB register span passes the 16-bit address space",
            ));
        }
        let deadline = now
            .checked_add(IO_TIMEOUT)
            .ok_or_else(|| invalid("USB register time overflow"))?;
        let words = u8::try_from(count).expect("invariant: callers bound counts to 64 words");
        let read = matches!(reply, Reply::Readback { .. });
        Ok(Self {
            id,
            frame: Frame::new(Command::encode(address, words, read), values),
            reply,
            phase: Phase::Queued,
            last: now,
            deadline,
            error: PhantomData,
        })
    }

    /// Hands the command to the executor once; subsequent transitions require events.
    pub(crate) fn start(&mut self, now: Duration) -> Result<Action, Failure<E>> {
        if self.phase != Phase::Queued {
            return self.fail(invalid(
                "USB register command was already started or terminated",
            ));
        }
        let timeout = self.remaining(now)?;
        self.phase = Phase::Writing;
        Ok(Action::Write {
            id: self.id,
            frame: self.frame,
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
            (Action::Write { id, frame, timeout }, Some(remaining))
                if id == self.id && self.phase == Phase::Writing =>
            {
                Ok(Action::Write {
                    id,
                    frame,
                    timeout: timeout.min(remaining),
                })
            }
            (Action::Readback { id, timeout }, Some(remaining))
                if id == self.id && matches!(self.phase, Phase::Readback { .. }) =>
            {
                Ok(Action::Readback {
                    id,
                    timeout: timeout.min(remaining),
                })
            }
            (Action::Notification { id, timeout }, Some(remaining))
                if id == self.id && matches!(self.phase, Phase::Notification { .. }) =>
            {
                Ok(Action::Notification {
                    id,
                    timeout: timeout.min(remaining),
                })
            }
            (complete @ Action::Complete { .. }, None) if self.phase == Phase::Complete => {
                Ok(complete)
            }
            _ => self.fail(invalid(
                "USB register action differs from the active transfer",
            )),
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
            return self.fail(invalid(
                "USB register completion identity differs from request",
            ));
        }
        if !matches!(
            self.phase,
            Phase::Writing | Phase::Readback { .. } | Phase::Notification { .. }
        ) {
            return self.fail(invalid(
                "USB register completion arrived outside an active transfer",
            ));
        }
        let remaining = self.remaining(now)?;
        match (event, self.phase) {
            (Event::Failed { error, .. }, _) => self.fail(Failure::Transport(error)),
            (Event::Written { .. }, Phase::Writing) => self.written(now),
            (Event::Readback { packet, .. }, Phase::Readback { count }) => {
                if packet.len() != count * 4 {
                    return self.fail(invalid("USB register readback length differs from request"));
                }
                let mut words = [0; MAX_READ_WORDS];
                for (word, bytes) in words.iter_mut().zip(packet.chunks_exact(4)) {
                    *word =
                        u32::from_le_bytes(bytes.try_into().expect("invariant: four-byte chunks"));
                }
                self.phase = Phase::Complete;
                Ok(Action::Complete { words, len: count })
            }
            (Event::Notification { packet, .. }, Phase::Notification { event }) => {
                match Notification::completion(packet, event, next_sequence) {
                    Ok(Some(value)) => {
                        self.phase = Phase::Complete;
                        let mut words = [0; MAX_READ_WORDS];
                        words[0] = value;
                        Ok(Action::Complete { words, len: 1 })
                    }
                    Ok(None) => Ok(Action::Notification {
                        id: self.id,
                        timeout: remaining,
                    }),
                    Err(reason) => self.fail(invalid(reason)),
                }
            }
            _ => self.fail(invalid(
                "USB register completion kind differs from transfer",
            )),
        }
    }

    /// The command completed: finish a write, or start the reply's own deadline.
    fn written(&mut self, now: Duration) -> Result<Action, Failure<E>> {
        let (phase, timeout) = match self.reply {
            Reply::None => {
                self.phase = Phase::Complete;
                return Ok(Action::Complete {
                    words: [0; MAX_READ_WORDS],
                    len: 0,
                });
            }
            Reply::Readback { count } => (Phase::Readback { count }, IO_TIMEOUT),
            Reply::Notification { event } => (Phase::Notification { event }, IRQ_TIMEOUT),
        };
        let Some(deadline) = now.checked_add(timeout) else {
            return self.fail(invalid("USB register time overflow"));
        };
        self.deadline = deadline;
        self.phase = phase;
        let id = self.id;
        Ok(match phase {
            Phase::Readback { .. } => Action::Readback { id, timeout },
            _ => Action::Notification { id, timeout },
        })
    }

    /// Checks monotonic elapsed time and the current phase deadline before any transition.
    fn remaining(&mut self, now: Duration) -> Result<Duration, Failure<E>> {
        if now < self.last {
            return self.fail(invalid("USB register time moved backwards"));
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

#[cfg(test)]
mod tests;
