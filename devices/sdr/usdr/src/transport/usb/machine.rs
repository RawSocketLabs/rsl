//! Both portable control machines behind one transfer vocabulary, so the blocking pump and
//! the native executor drive I²C, register and SPI operations the same way.

use std::time::Duration;

use super::i2c::{Action as I2cAction, Event, I2cTransaction};
use crate::lowlevel::BusError;
use crate::portable::usb::register::{
    Action as RegisterAction, MAX_READ_WORDS, RegisterTransaction,
};
use crate::portable::usb::transfer::Frame;

/// One control operation in flight: the I²C core, or register/SPI access.
#[expect(
    clippy::large_enum_variant,
    reason = "one per operation in flight; boxing would allocate per operation"
)]
pub(super) enum Machine {
    /// A simple-I²C command and its optional IRQ 6 readback.
    I2c(I2cTransaction),
    /// A register read, register write or SPI exchange.
    Register(RegisterTransaction<BusError>),
}

/// One endpoint transfer a machine asked for, with its timeout already refreshed.
#[derive(Debug)]
pub(super) struct Submission {
    /// The operation identity its completion must carry.
    pub id: u64,
    /// The transfer.
    pub transfer: Transfer,
    /// At most this long; the machine's phase deadline is never restarted.
    pub timeout: Duration,
}

/// The endpoint transfer to perform.
#[derive(Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "one command frame inline, so submitting never allocates"
)]
pub(super) enum Transfer {
    /// Send a command frame on endpoint 0x01.
    Write(Frame),
    /// Receive one whole register readback on endpoint 0x81.
    Readback,
    /// Receive one whole notification transfer on endpoint 0x82.
    Notification,
}

/// What a machine needs next.
#[derive(Debug)]
pub(super) enum Step {
    /// Perform this transfer and report its completion.
    Submit(Submission),
    /// The operation finished.
    Done(Output),
}

/// A finished operation's result.
#[derive(Clone, Copy, Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "returned once per operation; the word array is the register readback limit"
)]
pub(super) enum Output {
    /// I²C readback bytes in wire order; only the first `len` are valid.
    I2c { bytes: [u8; 4], len: usize },
    /// Registers read or the SPI reply; only the first `len` are valid (none for a write).
    Words {
        words: [u32; MAX_READ_WORDS],
        len: usize,
    },
}

impl Machine {
    /// Hands the command over once; the step is ready to submit.
    pub(super) fn start(&mut self, now: Duration) -> Result<Step, BusError> {
        match self {
            Self::I2c(transaction) => {
                let action = transaction.start(now)?;
                Self::i2c_step(transaction, action, now)
            }
            Self::Register(transaction) => {
                let action = transaction.start(now)?;
                Self::register_step(transaction, action, now)
            }
        }
    }

    /// Applies one correlated completion and returns what follows.
    pub(super) fn complete(
        &mut self,
        event: Event<'_>,
        now: Duration,
        next_sequence: &mut Option<u16>,
    ) -> Result<Step, BusError> {
        match self {
            Self::I2c(transaction) => {
                let action = transaction.complete(event, now, next_sequence)?;
                Self::i2c_step(transaction, action, now)
            }
            Self::Register(transaction) => {
                let action = transaction.complete(event, now, next_sequence)?;
                Self::register_step(transaction, action, now)
            }
        }
    }

    /// Refreshes an I²C action's timeout and translates it.
    fn i2c_step(
        transaction: &mut I2cTransaction,
        action: I2cAction,
        now: Duration,
    ) -> Result<Step, BusError> {
        Ok(match transaction.dispatch(action, now)? {
            I2cAction::Write {
                id,
                packet,
                timeout,
            } => Step::Submit(Submission {
                id,
                transfer: Transfer::Write(packet.into()),
                timeout,
            }),
            I2cAction::Notification { id, timeout } => Step::Submit(Submission {
                id,
                transfer: Transfer::Notification,
                timeout,
            }),
            I2cAction::Complete { bytes, len } => Step::Done(Output::I2c { bytes, len }),
        })
    }

    /// Refreshes a register action's timeout and translates it.
    fn register_step(
        transaction: &mut RegisterTransaction<BusError>,
        action: RegisterAction,
        now: Duration,
    ) -> Result<Step, BusError> {
        let (id, transfer, timeout) = match transaction.dispatch(action, now)? {
            RegisterAction::Write { id, frame, timeout } => (id, Transfer::Write(frame), timeout),
            RegisterAction::Readback { id, timeout } => (id, Transfer::Readback, timeout),
            RegisterAction::Notification { id, timeout } => (id, Transfer::Notification, timeout),
            RegisterAction::Complete { words, len } => {
                return Ok(Step::Done(Output::Words { words, len }));
            }
        };
        Ok(Step::Submit(Submission {
            id,
            transfer,
            timeout,
        }))
    }
}
