//! `lowlevel::Bus` over `xsdr-sim`, as libusdr's transports present the board.
//!
//! Shared by `xsdr`'s tests and the oracle's parity tests (`#[path]`-included), so both
//! drive the simulator through the same adapter. Limits and refusals are the oracle shim's
//! (`oracle/src/lib.rs`): at most three bytes written and four read per I2C transfer, and
//! SPI only on wired buses.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use xsdr::lowlevel::{
    Bus, BusError, I2C_MAX_READ, I2C_MAX_WRITE, I2cAddr, SpiAddr, TxCommit, TxFill,
};
use xsdr_sim::{I2cAddress, SimBoard};

/// The `PCIe` driver's largest DMA buffer, as the oracle's shim enforces it.
const MAX_DMA_BLOCK: u32 = 1 << 20;

/// A shared simulated board; clone it to inspect the board while a `Device` owns the bus.
#[derive(Clone)]
pub(crate) struct SimBus(Arc<Mutex<SimBoard>>);

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
        if dev.core() != 0 {
            return Err(BusError::Unsupported("the board has one I2C core"));
        }
        let addr = I2cAddress {
            bus: dev.bus(),
            addr: dev.addr().into(),
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

    /// As the oracle's shim: the `PCIe` driver's limit of 1 MiB per buffer, no tracing.
    fn rx_stream_open(&mut self, block_bytes: u32) -> Result<(), BusError> {
        if block_bytes > MAX_DMA_BLOCK {
            return Err(BusError::Unsupported(
                "RX block over the PCIe driver's 1 MiB",
            ));
        }
        Ok(())
    }

    /// As the oracle's shim: the board's next block, or a timeout at once while its stream
    /// engine is stopped.
    fn rx_stream_recv(
        &mut self,
        _timeout: Duration,
        consume: &mut dyn FnMut(&[u8], [u64; 2]),
    ) -> Result<(), BusError> {
        let Some((block, oob)) = self.board().rx_next_block() else {
            return Err(BusError::Timeout);
        };
        consume(&block, oob);
        Ok(())
    }

    fn rx_stream_close(&mut self) -> Result<(), BusError> {
        Ok(())
    }

    /// As the oracle's shim: the `PCIe` driver's limit of 1 MiB per buffer, then the
    /// board's 32 buffers set up.
    fn tx_stream_open(&mut self, block_bytes: u32, _sample_bytes: u32) -> Result<(), BusError> {
        if block_bytes > MAX_DMA_BLOCK {
            return Err(BusError::Unsupported(
                "TX block over the PCIe driver's 1 MiB",
            ));
        }
        let block_bytes = usize::try_from(block_bytes).expect("at most 1 MiB");
        self.board().tx_open(block_bytes);
        Ok(())
    }

    /// As the oracle's shim: the board's next free buffer, of the stream's block size, or
    /// a timeout at once while every buffer waits to play; the filled bytes go back to
    /// the board, which the driver's announcement then plays.
    fn tx_stream_send(
        &mut self,
        _timeout: Duration,
        _time: Option<u64>,
        fill: &mut TxFill<'_>,
    ) -> Result<TxCommit, BusError> {
        let mut board = self.board();
        let Some(record) = board.tx_grant() else {
            return Err(BusError::Timeout);
        };
        let mut buffer = vec![0; board.tx_block_bytes()];
        drop(board);
        let written = fill(&mut buffer, record.record());
        self.board().tx_load(&buffer[..written]);
        Ok(TxCommit::Registers)
    }

    fn tx_stream_close(&mut self) -> Result<(), BusError> {
        self.board().tx_close();
        Ok(())
    }
}
