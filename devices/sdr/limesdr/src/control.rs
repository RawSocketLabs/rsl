//! The control channel: what a transport (the FT601 USB bridge, or a simulator) must
//! provide.
//!
//! **Unstable.** The `LimeSDR`'s firmware answers LMS64C frames ([`lms64c`](crate::lms64c));
//! a transport moves one request and its reply at a time and keeps their order. Which
//! frames carry the board code's operations is the driver's business, not the
//! transport's. The same transport carries the sample streams, as blocks of whole 4 KiB
//! bursts the driver frames (libusdr's `stream_limesdr.c` over the transport's
//! `recv_dma_wait` and `send_dma_get`/`send_dma_commit`). No hardware transport exists
//! yet: libusdr's FT601 one is not in the pinned source.

use std::time::Duration;

pub use hal::BusError;

use crate::lms64c::FRAME_LEN;

/// One board's LMS64C control channel and sample streams, plus time.
pub trait ControlPort: Send {
    /// Sends one request frame and returns the board's reply frame.
    ///
    /// # Errors
    ///
    /// Any transport failure.
    fn exchange(&mut self, request: &[u8; FRAME_LEN]) -> Result<[u8; FRAME_LEN], BusError>;

    /// Waits for `duration`. Board code never reads the clock itself, so a simulator can
    /// make this virtual.
    fn sleep(&mut self, duration: Duration);

    /// Sets up the RX stream's transport for blocks of `block_bytes` (the transport's
    /// `stream_initialize`, stream 0). libusdr never tears a stream down, so neither does
    /// the driver: a later call replaces an earlier one.
    ///
    /// The default supports no streaming, for ports that only control the board.
    ///
    /// # Errors
    ///
    /// [`BusError::Unsupported`] by default; any transport failure.
    fn rx_stream_open(&mut self, block_bytes: u32) -> Result<(), BusError> {
        let _ = block_bytes;
        Err(BusError::Unsupported("RX streaming"))
    }

    /// Waits up to `timeout` for the next RX block and lends it to `consume` (the
    /// transport's `recv_dma_wait`, then `recv_dma_release` once the driver has copied it).
    ///
    /// # Errors
    ///
    /// [`BusError::Timeout`] if no block arrives in time; [`BusError::Unsupported`] by
    /// default; any transport failure.
    fn rx_stream_recv(
        &mut self,
        timeout: Duration,
        consume: &mut dyn FnMut(&[u8]),
    ) -> Result<(), BusError> {
        let _ = (timeout, consume);
        Err(BusError::Unsupported("RX streaming"))
    }

    /// Sets up the TX stream's transport for blocks of up to `block_bytes` (the
    /// transport's `stream_initialize`, stream 1), as [`Self::rx_stream_open`].
    ///
    /// # Errors
    ///
    /// [`BusError::Unsupported`] by default; any transport failure.
    fn tx_stream_open(&mut self, block_bytes: u32) -> Result<(), BusError> {
        let _ = block_bytes;
        Err(BusError::Unsupported("TX streaming"))
    }

    /// Waits up to `timeout` for a free TX buffer and lends it to `fill` (the transport's
    /// `send_dma_get`), which frames the block from the buffer's start and returns its
    /// length in bytes; then commits that many bytes (`send_dma_commit`) with `time`, the
    /// block's first sample time or `None` for as soon as possible, as its out-of-band
    /// word. The block's headers already carry both. When `fill` returns `None` (the
    /// buffer is too short for the block) nothing is committed and the buffer is kept.
    ///
    /// # Errors
    ///
    /// [`BusError::Timeout`] if no buffer frees in time; [`BusError::Unsupported`] by
    /// default; any transport failure.
    fn tx_stream_send(
        &mut self,
        timeout: Duration,
        time: Option<u64>,
        fill: &mut dyn FnMut(&mut [u8]) -> Option<usize>,
    ) -> Result<(), BusError> {
        let _ = (timeout, time, fill);
        Err(BusError::Unsupported("TX streaming"))
    }
}
