//! Queued USB TX blocks: libusdr's 16-byte header, then the samples, on endpoint 0x03.
//!
//! Source: `lowlevel/usb_uram/usb_uram_libusb.c` at oracle/libusdr/PIN
//! (`_usb_uram_init_txstream`, `usb_uram_send_dma_get`, `usb_uram_send_dma_commit`,
//! `usb_uram_stream_deinitialize`). The header carries the block's length and time, so
//! the gateware learns of a block from its transfer, not from the TX core's registers.

use std::time::Duration;

use rsl_usb::{Buffer, OutQueue};

use crate::lowlevel::{BusError, TxFill, usb_error};

/// libusdr's `MAX_OUT_STRM_REQS` (and the stream's `buffer_count`): transfers in flight.
const TRANSFERS: usize = 32;
/// `TX_PKT_HEADER` (`TXSTRM_META_SZ`): the header before each block's samples.
const HEADER: usize = 16;
/// `MAX_TX_BUFFER_SZ`: the largest block, the TX core's FIFO.
const MAX_BLOCK: usize = 126_976;
/// The smallest block `_usb_uram_init_txstream` takes; below it libusdr suspects an error.
const MIN_BLOCK: usize = 64;
/// `tx_stat_rate`: sends per read of the TX core's status words.
const STATUS_INTERVAL: u32 = 64;
/// The first of the TX core's four status words, which `usb_uram_send_dma_get` reads.
pub(super) const STATUS_REGISTER: u32 = 28;

/// Owned transfer boundary, implemented by the USB queue and hardware-free tests.
pub(super) trait TxQueue {
    /// Buffer returned only after its transfer has completed.
    type Buffer;
    /// The whole buffer, to fill.
    fn bytes(buffer: &mut Self::Buffer) -> &mut [u8];
    /// Allocates one transfer buffer of `capacity` bytes.
    fn alloc(&mut self, capacity: usize) -> Result<Self::Buffer, BusError>;
    /// Sends the first `len` bytes of this buffer.
    fn submit(&mut self, buffer: Self::Buffer, len: usize) -> Result<(), BusError>;
    /// Waits for the oldest transfer without cancelling it on timeout; a failed
    /// transfer is an error.
    fn wait(&mut self, timeout: Duration) -> Result<Option<Self::Buffer>, BusError>;
    /// Cancels and drains every outstanding transfer.
    fn cancel(&mut self) -> Result<(), BusError>;
}

impl TxQueue for OutQueue {
    type Buffer = Buffer;

    fn bytes(buffer: &mut Buffer) -> &mut [u8] {
        buffer.as_mut_slice()
    }

    fn alloc(&mut self, capacity: usize) -> Result<Buffer, BusError> {
        Ok(self.alloc(capacity))
    }

    fn submit(&mut self, mut buffer: Buffer, len: usize) -> Result<(), BusError> {
        buffer.set_len(len);
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

/// The block sizes of an open stream.
#[derive(Clone, Copy, Debug)]
struct Shape {
    /// The most sample bytes one block takes, excluding the header.
    block_bytes: usize,
    /// Bytes per sample across all channels (libusdr's `bits_per_all_chs` / 8).
    sample_bytes: usize,
}

/// One TX endpoint; a failed stream must be drained before it can be restarted.
pub(super) struct Tx<Q: TxQueue> {
    /// Owns in-flight transfers even after a failure.
    queue: Q,
    /// The block sizes while open.
    shape: Option<Shape>,
    /// Transfers submitted and not yet waited for.
    in_flight: usize,
    /// The buffer the next commit fills, taken by [`Tx::acquire`].
    ready: Option<Q::Buffer>,
    /// Failed transfer or incomplete cancellation.
    failed: bool,
}

impl<Q: TxQueue> Tx<Q> {
    /// Opens an idle endpoint, draining failed setup or retaining ownership if drain fails.
    /// Endpoint acquisition occurs only after rejecting an existing stream.
    pub(super) fn open_endpoint(
        slot: &mut Option<Self>,
        block_bytes: u32,
        sample_bytes: u32,
        acquire: impl FnOnce() -> Result<Q, BusError>,
    ) -> Result<(), BusError> {
        if slot.is_some() {
            return Err(BusError::Unsupported(
                "USB TX already open or awaiting cleanup",
            ));
        }
        let shape = Self::shape(block_bytes, sample_bytes)?;
        let tx = slot.insert(Self::new(acquire()?));
        tx.shape = Some(shape);
        Ok(())
    }

    /// Releases an endpoint only after cancellation returns all its transfers.
    pub(super) fn close_endpoint(slot: &mut Option<Self>) -> Result<(), BusError> {
        if let Some(tx) = slot.as_mut() {
            tx.close()?;
            *slot = None;
        }
        Ok(())
    }

    /// Takes ownership of an idle endpoint.
    pub(super) fn new(queue: Q) -> Self {
        Self {
            queue,
            shape: None,
            in_flight: 0,
            ready: None,
            failed: false,
        }
    }

    /// Checks a block size as `_usb_uram_init_txstream` does: 64 bytes to
    /// `MAX_TX_BUFFER_SZ`. Intentional divergence: libusdr takes 0 for the largest; the
    /// driver never asks for it, so it is refused.
    fn shape(block_bytes: u32, sample_bytes: u32) -> Result<Shape, BusError> {
        let block_bytes = usize::try_from(block_bytes)
            .map_err(|_| BusError::Unsupported("USB TX block does not fit usize"))?;
        if !(MIN_BLOCK..=MAX_BLOCK).contains(&block_bytes) {
            return Err(BusError::Unsupported(
                "USB TX block must be 64 to 126976 bytes",
            ));
        }
        let sample_bytes = usize::try_from(sample_bytes)
            .ok()
            .filter(|&bytes| bytes > 0)
            .ok_or(BusError::Unsupported("USB TX samples must take bytes"))?;
        Ok(Shape {
            block_bytes,
            sample_bytes,
        })
    }

    /// The open stream's shape, unless it failed.
    fn usable(&self) -> Result<Shape, BusError> {
        self.shape
            .filter(|_| !self.failed)
            .ok_or(BusError::Unsupported("USB TX is closed or failed"))
    }

    /// Takes a buffer for the next commit (`usb_uram_send_dma_get`'s wait): a new one
    /// while fewer than 32 transfers are in flight, else the oldest once it completes,
    /// waiting up to `timeout`. A timeout leaves every transfer in flight.
    pub(super) fn acquire(&mut self, timeout: Duration) -> Result<(), BusError> {
        let shape = self.usable()?;
        if self.ready.is_some() {
            return Ok(());
        }
        let buffer = if self.in_flight < TRANSFERS {
            self.queue.alloc(HEADER + shape.block_bytes)?
        } else {
            match self.queue.wait(timeout) {
                Ok(Some(buffer)) => {
                    self.in_flight -= 1;
                    buffer
                }
                Ok(None) => return Err(BusError::Timeout),
                Err(error) => {
                    self.failed = true;
                    return Err(error);
                }
            }
        };
        self.ready = Some(buffer);
        Ok(())
    }

    /// Lends the acquired buffer's sample area to `fill` with `status`, then frames the
    /// block it wrote with `time` (`None`: as soon as it arrives) and queues it
    /// (`usb_uram_send_dma_commit`).
    ///
    /// A block of no bytes, more than the stream's or not whole samples is refused and
    /// nothing is sent; libusdr frames whatever length it gets up to the buffer.
    pub(super) fn commit(
        &mut self,
        time: Option<u64>,
        status: Option<[u32; 4]>,
        fill: &mut TxFill<'_>,
    ) -> Result<(), BusError> {
        let shape = self.usable()?;
        let Some(mut buffer) = self.ready.take() else {
            return Err(BusError::Unsupported("no USB TX buffer acquired"));
        };
        let bytes = Q::bytes(&mut buffer);
        let len = fill(&mut bytes[HEADER..HEADER + shape.block_bytes], status);
        if len == 0 || len > shape.block_bytes || len % shape.sample_bytes != 0 {
            self.ready = Some(buffer);
            return Err(BusError::Unsupported(
                "USB TX block must be whole samples, 1 to the stream's block",
            ));
        }
        bytes[..HEADER].copy_from_slice(&header(time, len, shape.sample_bytes));
        if let Err(error) = self.queue.submit(buffer, HEADER + len) {
            self.failed = true;
            return Err(error);
        }
        self.in_flight += 1;
        Ok(())
    }

    /// Cancels every transfer, sent or not (libusdr's `buffers_usb_free`); a failed
    /// drain remains retryable.
    pub(super) fn close(&mut self) -> Result<(), BusError> {
        if let Err(error) = self.queue.cancel() {
            self.failed = true;
            return Err(error);
        }
        self.shape = None;
        self.in_flight = 0;
        self.ready = None;
        self.failed = false;
        Ok(())
    }
}

/// `usb_uram_send_dma_commit`'s header for a block of `len` bytes, samples of
/// `sample_bytes`, to play at `time` (`None`: libusdr's -1, as soon as it arrives).
///
/// Little-endian words: the time's bits 31:0; its bits 47:32, samples less one in bits
/// 30:16 and "now" in bit 31; bytes less one in bits 31:15 and the time's bits 62:48;
/// zero. libusdr notes the old TX core ignores the third word and the new one the
/// sample count.
fn header(time: Option<u64>, len: usize, sample_bytes: usize) -> [u8; HEADER] {
    let stamp = time.unwrap_or(u64::MAX);
    let field = |value: u64, mask: u64| u32::try_from(value & mask).expect("masked to 32 bits");
    let samples_less_one = u64::try_from(len / sample_bytes - 1).expect("a block fits u64");
    let bytes_less_one = u64::try_from(len - 1).expect("a block fits u64");
    let words = [
        field(stamp, 0xffff_ffff),
        field(stamp >> 32, 0xffff)
            | field(samples_less_one, 0x7fff) << 16
            | field(stamp >> 63, 1) << 31,
        field(bytes_less_one, 0x1_ffff) << 15 | field(stamp >> 48, 0x7fff),
        0,
    ];
    let mut header = [0; HEADER];
    for (bytes, word) in header.chunks_exact_mut(4).zip(words) {
        bytes.copy_from_slice(&word.to_le_bytes());
    }
    header
}

/// libusdr's throttled TX status (`tx_stat_cnt`, `tx_stat_rate`, `tx_stat_prev`): every
/// send reports the TX core's four status words, read afresh only on every 64th send
/// from the device's first, so the rest repeat the last reading. Per device, not per
/// stream, as in libusdr.
#[derive(Debug, Default)]
pub(super) struct TxStatus {
    /// Sends counted so far, wrapping as libusdr's `unsigned` does.
    sends: u32,
    /// The last reading; zeros before the first.
    words: [u32; 4],
}

impl TxStatus {
    /// Counts one send and says whether it reads the status words afresh.
    pub(super) fn count_send(&mut self) -> bool {
        let sends = self.sends;
        self.sends = sends.wrapping_add(1);
        sends % STATUS_INTERVAL == 0
    }

    /// Keeps a fresh reading.
    pub(super) fn record(&mut self, words: [u32; 4]) {
        self.words = words;
    }

    /// The words this send reports.
    pub(super) fn words(&self) -> [u32; 4] {
        self.words
    }
}

#[cfg(test)]
#[path = "tx_tests.rs"]
mod tests;
