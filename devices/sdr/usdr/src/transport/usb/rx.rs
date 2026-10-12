//! Queued USB RX blocks: raw samples followed by libusdr's 16-byte trailer.

use std::io;
use std::time::Duration;

use rsl_usb::{Buffer, InQueue};

use crate::lowlevel::{BusError, usb_error};

/// Linux libusdr's `MAX_IN_STRM_REQS`; keep eight reusable transfers in flight.
const TRANSFERS: usize = 8;
/// Matches the driver's existing maximum DMA block; bounds allocations to about 8 MiB.
const MAX_BLOCK: usize = 1 << 20;
/// `RX_PKT_TRAILER_EX`, at oracle/libusdr/PIN.
const TRAILER: usize = 16;

/// Owned transfer boundary, implemented by the USB queue and hardware-free tests.
pub(super) trait RxQueue {
    /// Buffer returned only after its transfer has completed.
    type Buffer;
    /// Completed bytes, excluding unused buffer capacity.
    fn data(buffer: &Self::Buffer) -> &[u8];
    /// Endpoint maximum packet size.
    fn packet_size(&self) -> usize;
    /// Allocates one packet-aligned transfer buffer.
    fn alloc(&mut self, capacity: usize) -> Result<Self::Buffer, BusError>;
    /// Returns this buffer to the endpoint.
    fn submit(&mut self, buffer: Self::Buffer) -> Result<(), BusError>;
    /// Waits for the oldest transfer without cancelling it on timeout.
    fn wait(&mut self, timeout: Duration) -> Result<Option<Self::Buffer>, BusError>;
    /// Cancels and drains every outstanding transfer.
    fn cancel(&mut self) -> Result<(), BusError>;
}

impl RxQueue for InQueue {
    type Buffer = Buffer;

    fn data(buffer: &Buffer) -> &[u8] {
        buffer
    }

    fn packet_size(&self) -> usize {
        self.max_packet_size()
    }

    fn alloc(&mut self, capacity: usize) -> Result<Buffer, BusError> {
        self.alloc(capacity).map_err(usb_error)
    }

    fn submit(&mut self, buffer: Buffer) -> Result<(), BusError> {
        self.submit(buffer)
            .map_err(|rejected| usb_error(rejected.error))
    }

    fn wait(&mut self, timeout: Duration) -> Result<Option<Buffer>, BusError> {
        let Some(done) = self.wait(timeout).map_err(usb_error)? else {
            return Ok(None);
        };
        done.status
            .map_err(rsl_usb::Error::from)
            .map_err(usb_error)?;
        Ok(Some(done.buffer))
    }

    fn cancel(&mut self) -> Result<(), BusError> {
        self.cancel_all().map(|_| ()).map_err(usb_error)
    }
}

/// One RX endpoint; a failed stream must be drained before it can be restarted.
pub(super) struct Rx<Q> {
    /// Owns pending transfers even after a failure.
    queue: Q,
    /// Exact sample-block length while open, excluding the USB trailer.
    block_bytes: Option<usize>,
    /// Failed transfer, malformed frame, or incomplete cancellation.
    failed: bool,
}

impl<Q: RxQueue> Rx<Q> {
    /// Opens an idle endpoint, draining failed setup or retaining ownership if drain fails.
    /// Endpoint acquisition occurs only after rejecting an existing stream.
    pub(super) fn open_endpoint(
        slot: &mut Option<Self>,
        block_bytes: u32,
        acquire: impl FnOnce() -> Result<Q, BusError>,
    ) -> Result<(), BusError> {
        if slot.is_some() {
            return Err(BusError::Unsupported(
                "USB RX already open or awaiting cleanup",
            ));
        }
        let rx = slot.insert(Self::new(acquire()?));
        if let Err(error) = rx.open(block_bytes) {
            // Keep ownership if cancellation cannot return every posted transfer.
            if rx.close().is_ok() {
                *slot = None;
            }
            return Err(error);
        }
        Ok(())
    }

    /// Releases an endpoint only after cancellation returns all its transfers.
    pub(super) fn close_endpoint(slot: &mut Option<Self>) -> Result<(), BusError> {
        if let Some(rx) = slot.as_mut() {
            rx.close()?;
            *slot = None;
        }
        Ok(())
    }

    /// Takes ownership of an idle endpoint.
    pub(super) fn new(queue: Q) -> Self {
        Self {
            queue,
            block_bytes: None,
            failed: false,
        }
    }

    /// Primes the bounded ring before the board starts producing samples.
    pub(super) fn open(&mut self, block_bytes: u32) -> Result<(), BusError> {
        if self.block_bytes.is_some() || self.failed {
            return Err(BusError::Unsupported(
                "USB RX already open or awaiting cleanup",
            ));
        }
        let block_bytes = usize::try_from(block_bytes)
            .map_err(|_| BusError::Unsupported("USB RX block does not fit usize"))?;
        if block_bytes == 0 || block_bytes > MAX_BLOCK || block_bytes % 8 != 0 {
            return Err(BusError::Unsupported(
                "USB RX block must be whole words, at most 1 MiB",
            ));
        }
        let packet = self.queue.packet_size();
        if !matches!(packet, 512 | 1024) {
            return Err(BusError::Unsupported(
                "USB RX requires a high-speed bulk endpoint",
            ));
        }
        // Leave room for a terminating ZLP when payload+trailer is packet-aligned.
        // Otherwise the full frame ends this URB and its ZLP completes the next one empty.
        let capacity = ((block_bytes + TRAILER) / packet + 1) * packet;
        self.block_bytes = Some(block_bytes);
        for _ in 0..TRANSFERS {
            let posted = self
                .queue
                .alloc(capacity)
                .and_then(|buffer| self.queue.submit(buffer));
            if let Err(error) = posted {
                self.failed = true;
                return Err(error);
            }
        }
        Ok(())
    }

    /// Lends one complete sample block, then reuses its buffer; timeout leaves it pending.
    pub(super) fn receive(
        &mut self,
        timeout: Duration,
        consume: &mut dyn FnMut(&[u8], [u64; 2]),
    ) -> Result<(), BusError> {
        let Some(block_bytes) = self.block_bytes.filter(|_| !self.failed) else {
            return Err(BusError::Unsupported("USB RX is closed or failed"));
        };
        let buffer = match self.queue.wait(timeout) {
            Ok(Some(buffer)) => buffer,
            Ok(None) => return Err(BusError::Timeout),
            Err(error) => {
                self.failed = true;
                return Err(error);
            }
        };
        let frame = Q::data(&buffer);
        if frame.len() != block_bytes + TRAILER {
            self.failed = true;
            return Err(BusError::Io(io::Error::new(
                io::ErrorKind::InvalidData,
                "USB RX transfer must contain one block and its trailer",
            )));
        }
        let (payload, trailer) = frame.split_at(block_bytes);
        // usb_uram_recv_dma_wait copies the first two trailer words unchanged into OOB0.
        // Despite C's names `bursts`/`skipped`, low24 is loss and high32 is the burst mask.
        let word = |offset| {
            u64::from_le_bytes(
                trailer[offset..offset + 8]
                    .try_into()
                    .expect("invariant: the validated trailer contains two u64 words"),
            )
        };
        consume(payload, [word(0), word(8)]);
        if let Err(error) = self.queue.submit(buffer) {
            self.failed = true;
            return Err(error);
        }
        Ok(())
    }

    /// Drains RX independently of control-plane health; a failed drain remains retryable.
    pub(super) fn close(&mut self) -> Result<(), BusError> {
        if let Err(error) = self.queue.cancel() {
            self.failed = true;
            return Err(error);
        }
        self.block_bytes = None;
        self.failed = false;
        Ok(())
    }
}

#[cfg(test)]
#[path = "rx_tests.rs"]
mod tests;
