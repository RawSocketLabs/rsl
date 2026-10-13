//! A `Bus` over the simulated board that fails one chosen operation, to test what the driver
//! does after a bus error. The failed operation never reaches the board, so it is missing
//! from the trace; every other operation passes through.

use std::time::Duration;

use rsl_pe_sync::lowlevel::{Bus, BusError, I2cAddr, SpiAddr};

use super::sim_bus::SimBus;

/// One bus operation, as a failure predicate sees it.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Access {
    /// A gateware register write: address and value.
    Write(u32, u32),
    /// An SPI word: bus and word.
    Spi(u8, u32),
}

/// [`SimBus`], failing the `skip + 1`th operation `matches` accepts with
/// [`BusError::Disconnected`].
pub(crate) struct FailingBus {
    /// The board.
    inner: SimBus,
    /// Which operations count.
    matches: fn(Access) -> bool,
    /// How many matching operations still pass before the failure.
    skip: usize,
    /// The failure happened.
    failed: bool,
}

impl FailingBus {
    /// Fails the `skip + 1`th operation of `inner` that `matches` accepts.
    pub(crate) fn new(inner: SimBus, matches: fn(Access) -> bool, skip: usize) -> Self {
        Self {
            inner,
            matches,
            skip,
            failed: false,
        }
    }

    /// Whether `access` is the one to fail.
    fn fails(&mut self, access: Access) -> bool {
        if self.failed || !(self.matches)(access) {
            return false;
        }
        if self.skip > 0 {
            self.skip -= 1;
            return false;
        }
        self.failed = true;
        true
    }
}

impl Bus for FailingBus {
    fn read_regs(&mut self, addr: u32, out: &mut [u32]) -> Result<(), BusError> {
        self.inner.read_regs(addr, out)
    }

    fn write_regs(&mut self, addr: u32, values: &[u32]) -> Result<(), BusError> {
        if values
            .iter()
            .any(|&value| self.fails(Access::Write(addr, value)))
        {
            return Err(BusError::Disconnected);
        }
        self.inner.write_regs(addr, values)
    }

    fn spi32(&mut self, target: SpiAddr, word: u32) -> Result<u32, BusError> {
        if self.fails(Access::Spi(target.bus(), word)) {
            return Err(BusError::Disconnected);
        }
        self.inner.spi32(target, word)
    }

    fn i2c(&mut self, dev: I2cAddr, write: &[u8], read: &mut [u8]) -> Result<(), BusError> {
        self.inner.i2c(dev, write, read)
    }

    fn sleep(&mut self, duration: Duration) {
        self.inner.sleep(duration);
    }
}
