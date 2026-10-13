//! The one-action sequences the chip's composite machines are built from, and how an
//! action's outcome becomes a result.

use core::time::Duration;

use rsl_lowlevel::{FpgaAccess, Sequence, SequenceAction, SequenceFailure, SpiAddr};

use super::NAME;
use super::reg::Reg;
use super::spi;

/// The word `action` reported, or the failure that ends the sequence. A wait cannot fail:
/// its outcome only says it has passed.
///
/// # Errors
///
/// The executor's error, with the access it struck.
pub(crate) fn checked<E>(
    action: SequenceAction,
    result: Result<u32, E>,
) -> Result<u32, SequenceFailure<E>> {
    result.or_else(|source| match action {
        SequenceAction::Sleep(_) => Ok(0),
        SequenceAction::Fpga(access) => Err(SequenceFailure::FpgaTransport {
            chip: NAME,
            access,
            source,
        }),
        SequenceAction::Access(access) => Err(SequenceFailure::Transport {
            chip: NAME,
            access,
            source,
        }),
    })
}

/// One wait: libusdr's `usleep` between chip accesses.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Wait {
    /// How long.
    duration: Duration,

    /// Waited already.
    done: bool,
}

impl Wait {
    /// A wait of `micros` microseconds.
    pub(crate) const fn micros(micros: u64) -> Self {
        Self {
            duration: Duration::from_micros(micros),
            done: false,
        }
    }
}

impl Sequence for Wait {
    fn next(&self) -> Option<SequenceAction> {
        (!self.done).then_some(SequenceAction::Sleep(self.duration))
    }

    fn complete<E>(&mut self, _result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        self.done = true;
        Ok(())
    }
}

/// One register read: libusdr's `lms7002m_spi_rd`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Read {
    /// The exchange.
    access: FpgaAccess,

    /// The value read, once read.
    value: Option<u16>,

    /// The read failed.
    failed: bool,
}

impl Read {
    /// A read of `reg` on the chip at `target`.
    pub(crate) fn new(target: SpiAddr, reg: Reg) -> Self {
        Self {
            access: spi::read(target, reg),
            value: None,
            failed: false,
        }
    }

    /// The register's value, once read.
    pub(crate) const fn value(&self) -> Option<u16> {
        self.value
    }
}

impl Sequence for Read {
    fn next(&self) -> Option<SequenceAction> {
        (self.value.is_none() && !self.failed).then_some(SequenceAction::Fpga(self.access))
    }

    fn complete<E>(&mut self, result: Result<u32, E>) -> Result<(), SequenceFailure<E>> {
        let Some(action) = self.next() else {
            return Ok(());
        };
        let word = checked(action, result).inspect_err(|_| self.failed = true)?;
        self.value = Some(spi::value(word));
        Ok(())
    }
}
