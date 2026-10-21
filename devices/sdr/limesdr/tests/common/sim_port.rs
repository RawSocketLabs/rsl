//! `control::ControlPort` over `limesdr-sim`: every frame goes to the board's LMS64C
//! handler and every wait to its virtual clock.
//!
//! Shared by `limesdr`'s tests and the oracle's parity tests (`#[path]`-included), so
//! both drive the simulator through the same adapter.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use limesdr::control::{BusError, ControlPort};
use limesdr::lms64c::FRAME_LEN;
use limesdr_sim::SimBoard;

/// A shared simulated board; clone it to inspect the board while a `Device` owns the port.
#[derive(Clone)]
pub(crate) struct SimPort(Arc<Mutex<SimBoard>>);

impl SimPort {
    /// Wraps a board.
    pub(crate) fn new(board: SimBoard) -> Self {
        Self(Arc::new(Mutex::new(board)))
    }

    /// Locks the board.
    pub(crate) fn board(&self) -> MutexGuard<'_, SimBoard> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl ControlPort for SimPort {
    fn exchange(&mut self, request: &[u8; FRAME_LEN]) -> Result<[u8; FRAME_LEN], BusError> {
        Ok(self.board().lms64c(request))
    }

    fn sleep(&mut self, duration: Duration) {
        let us = u64::try_from(duration.as_micros())
            .expect("invariant: board delays fit in u64 microseconds");
        self.board().sleep_us(us);
    }

    fn rx_stream_open(&mut self, block_bytes: u32) -> Result<(), BusError> {
        self.board().stream_open(false, block_bytes);
        Ok(())
    }

    fn rx_stream_recv(
        &mut self,
        _timeout: Duration,
        consume: &mut dyn FnMut(&[u8]),
    ) -> Result<(), BusError> {
        let block = self.board().rx_block().ok_or(BusError::Timeout)?;
        consume(&block);
        Ok(())
    }

    fn tx_stream_open(&mut self, block_bytes: u32) -> Result<(), BusError> {
        self.board().stream_open(true, block_bytes);
        Ok(())
    }

    fn tx_stream_send(
        &mut self,
        _timeout: Duration,
        time: Option<u64>,
        fill: &mut dyn FnMut(&mut [u8]) -> usize,
    ) -> Result<(), BusError> {
        let mut board = self.board();
        let Some(capacity) = board.tx_capacity() else {
            return Err(BusError::Unsupported("no TX stream"));
        };
        let mut buffer = vec![0; capacity];
        let len = fill(&mut buffer);
        if board.tx_block(&buffer[..len], time) {
            Ok(())
        } else {
            Err(BusError::Unsupported("TX block over the stream's size"))
        }
    }
}
