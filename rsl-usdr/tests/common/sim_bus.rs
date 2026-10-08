//! `lowlevel::Bus` over `rsl-usdr-sim`, as libusdr's real transports present the board.
//!
//! Shared by `rsl-usdr`'s tests and the oracle's parity tests (`#[path]`-included), so both
//! drive the simulator through the same adapter.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use rsl_usdr::lowlevel::{Bus, BusError, I2C_MAX_READ, I2C_MAX_WRITE, I2cAddr, SpiAddr};
use rsl_usdr_sim::{I2cAddress, SimBoard};

/// A shared simulated board; clone it to inspect the board while a `Device` owns the bus.
#[derive(Clone)]
pub(crate) struct SimBus(pub(crate) Arc<Mutex<SimBoard>>);

impl SimBus {
    /// Wraps a board.
    pub(crate) fn new(board: SimBoard) -> Self {
        Self(Arc::new(Mutex::new(board)))
    }

    /// Locks the board.
    pub(crate) fn board(&self) -> MutexGuard<'_, SimBoard> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Bus for SimBus {
    fn read_regs(&mut self, addr: u32, out: &mut [u32]) -> Result<(), BusError> {
        let mut board = self.board();
        for (reg, value) in (addr..).zip(out) {
            *value = board.read_reg(reg);
        }
        Ok(())
    }

    fn write_regs(&mut self, addr: u32, values: &[u32]) -> Result<(), BusError> {
        let mut board = self.board();
        for (reg, &value) in (addr..).zip(values) {
            board.write_reg(reg, value);
        }
        Ok(())
    }

    fn spi32(&mut self, target: SpiAddr, word: u32) -> Result<u32, BusError> {
        self.board()
            .spi(target.bus().into(), word)
            .map_err(|err| BusError::Other(Box::new(err)))
    }

    fn i2c(&mut self, dev: I2cAddr, write: &[u8], read: &mut [u8]) -> Result<(), BusError> {
        if write.len() > I2C_MAX_WRITE || read.len() > I2C_MAX_READ {
            return Err(BusError::Unsupported(
                "I2C transfer over the FPGA core limits",
            ));
        }
        let addr = I2cAddress {
            bus: dev.bus,
            addr: dev.addr.into(),
        };
        let bytes = self.board().i2c(addr, write, read.len());
        read.copy_from_slice(&bytes);
        Ok(())
    }

    fn sleep(&mut self, duration: Duration) {
        let us = u64::try_from(duration.as_micros())
            .expect("invariant: board delays fit in u64 microseconds");
        self.board().sleep_us(us);
    }
}
