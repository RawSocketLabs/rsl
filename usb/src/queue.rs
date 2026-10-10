//! Endpoint queues: transfers submitted on one endpoint and handed back in order.

use std::any::Any;
use std::collections::VecDeque;
use std::ptr::NonNull;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::buffer::Buffer;
use crate::dispatch::{Dispatcher, Transfer};
use crate::error::{Error, TransferError};
use crate::usbfs::Urb;

/// How long dropping a queue waits for its cancelled transfers to come back. Past it they are
/// leaked rather than freed under the kernel, and the endpoint stays unavailable.
const DROP_GRACE: Duration = Duration::from_secs(5);

/// A finished transfer: its buffer, back from the kernel, and how it ended.
#[derive(Debug)]
#[must_use]
pub struct Completion {
    /// The buffer; for an IN transfer, its data is what arrived.
    pub buffer: Buffer,

    /// How the transfer ended.
    pub status: Result<(), TransferError>,
}

/// A transfer the kernel would not take, with its buffer handed back.
#[derive(Debug, thiserror::Error)]
#[error("transfer not submitted: {error}")]
pub struct Rejected {
    /// Why.
    #[source]
    pub error: Error,

    /// The buffer, unchanged.
    pub buffer: Buffer,
}

/// A queue of IN transfers (device to host) on one bulk or interrupt endpoint.
#[derive(Debug)]
pub struct InQueue(Pipe);

/// A queue of OUT transfers (host to device) on one bulk or interrupt endpoint.
#[derive(Debug)]
pub struct OutQueue(Pipe);

impl InQueue {
    /// Wraps a pipe on an IN endpoint.
    pub(crate) fn new(pipe: Pipe) -> Self {
        Self(pipe)
    }

    /// The endpoint's packet size; IN transfer lengths are whole multiples of it.
    #[must_use]
    pub fn max_packet_size(&self) -> usize {
        self.0.max_packet
    }

    /// Transfers submitted and not yet handed back.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.0.pending.len()
    }

    /// A buffer of `len` bytes for this queue, mapped from the device where possible.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidLength`] unless `len` is a nonzero multiple of the packet size.
    pub fn alloc(&mut self, len: usize) -> Result<Buffer, Error> {
        self.0.check_in_length(len)?;
        Ok(self.0.alloc(len))
    }

    /// Queues a transfer filling `buffer` to its capacity, or less if the device ends it
    /// early with a short packet.
    ///
    /// # Errors
    ///
    /// The buffer back, with [`Error::InvalidLength`] if its capacity is not a nonzero
    /// multiple of the packet size, or why the kernel refused it.
    pub fn submit(&mut self, mut buffer: Buffer) -> Result<(), Rejected> {
        if let Err(error) = self.0.check_in_length(buffer.capacity()) {
            return Err(Rejected { error, buffer });
        }
        let capacity = buffer.capacity();
        buffer.set_len(capacity);
        self.0.submit(buffer)
    }

    /// The oldest transfer once it completes, waiting up to `timeout`; `None` if it is still
    /// in flight then, or nothing is queued.
    ///
    /// # Errors
    ///
    /// [`Error::Disconnected`] once the device has gone and nothing is left to hand back.
    pub fn wait(&mut self, timeout: Duration) -> Result<Option<Completion>, Error> {
        self.0.wait(timeout)
    }

    /// Cancels every queued transfer and waits for them to come back, returning their
    /// buffers.
    ///
    /// # Errors
    ///
    /// [`Error::TimedOut`] if the kernel has not returned them all within a few seconds;
    /// those still out stay queued.
    pub fn cancel_all(&mut self) -> Result<Vec<Buffer>, Error> {
        self.0.cancel_all(DROP_GRACE)
    }

    /// Reads one transfer of up to `data.len()` bytes, waiting up to `timeout`; returns the
    /// bytes read. For occasional reads, such as status or notification endpoints.
    ///
    /// # Errors
    ///
    /// [`Error::Busy`] if transfers are queued, [`Error::TimedOut`], or how the transfer
    /// failed.
    pub fn read(&mut self, data: &mut [u8], timeout: Duration) -> Result<usize, Error> {
        if !self.0.pending.is_empty() {
            return Err(Error::Busy);
        }
        let max_packet = self.0.max_packet;
        let len = data.len().div_ceil(max_packet).max(1) * max_packet;
        let buffer = self.0.alloc(len);
        self.submit(buffer).map_err(|rejected| rejected.error)?;
        let done = self.0.finish(timeout)?;
        done.status?;
        let read = done.buffer.len().min(data.len());
        data[..read].copy_from_slice(&done.buffer[..read]);
        Ok(read)
    }
}

impl OutQueue {
    /// Wraps a pipe on an OUT endpoint.
    pub(crate) fn new(pipe: Pipe) -> Self {
        Self(pipe)
    }

    /// The endpoint's packet size.
    #[must_use]
    pub fn max_packet_size(&self) -> usize {
        self.0.max_packet
    }

    /// Transfers submitted and not yet handed back.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.0.pending.len()
    }

    /// An empty buffer that holds up to `capacity` bytes, mapped from the device where
    /// possible.
    #[must_use]
    pub fn alloc(&mut self, capacity: usize) -> Buffer {
        self.0.alloc(capacity)
    }

    /// Queues a transfer sending `buffer`'s data.
    ///
    /// # Errors
    ///
    /// The buffer back, with why the kernel refused it.
    pub fn submit(&mut self, buffer: Buffer) -> Result<(), Rejected> {
        self.0.submit(buffer)
    }

    /// The oldest transfer once it completes, waiting up to `timeout`; `None` if it is still
    /// in flight then, or nothing is queued.
    ///
    /// # Errors
    ///
    /// [`Error::Disconnected`] once the device has gone and nothing is left to hand back.
    pub fn wait(&mut self, timeout: Duration) -> Result<Option<Completion>, Error> {
        self.0.wait(timeout)
    }

    /// Cancels every queued transfer and waits for them to come back, returning their
    /// buffers.
    ///
    /// # Errors
    ///
    /// [`Error::TimedOut`] if the kernel has not returned them all within a few seconds;
    /// those still out stay queued.
    pub fn cancel_all(&mut self) -> Result<Vec<Buffer>, Error> {
        self.0.cancel_all(DROP_GRACE)
    }

    /// Sends `data` as one transfer, waiting up to `timeout` for it to complete.
    ///
    /// # Errors
    ///
    /// [`Error::Busy`] if transfers are queued, [`Error::TimedOut`], or how the transfer
    /// failed.
    pub fn write(&mut self, data: &[u8], timeout: Duration) -> Result<(), Error> {
        if !self.0.pending.is_empty() {
            return Err(Error::Busy);
        }
        let mut buffer = self.0.alloc(data.len());
        buffer.as_mut_slice()[..data.len()].copy_from_slice(data);
        buffer.set_len(data.len());
        self.submit(buffer).map_err(|rejected| rejected.error)?;
        Ok(self.0.finish(timeout)?.status?)
    }
}

/// What both queue directions share: the endpoint, its in-flight transfers, and the device.
pub(crate) struct Pipe {
    /// The open device's completions.
    dispatcher: Arc<Dispatcher>,

    /// Keeps the interface claimed while the queue exists.
    _interface: Arc<dyn Any + Send + Sync>,

    /// The endpoint address, direction bit included.
    endpoint: u8,

    /// `USBDEVFS_URB_TYPE_*`.
    kind: u8,

    /// The endpoint's packet size.
    max_packet: usize,

    /// Transfers in flight, oldest first.
    pending: VecDeque<NonNull<Transfer>>,
}

// SAFETY: the in-flight transfers are owned by the kernel and by this pipe alone; the pipe
// touches them only through `&mut self`, so moving it moves all access with it.
unsafe impl Send for Pipe {}
// SAFETY: `&Pipe` exposes only the endpoint's facts and the pending count, never the
// transfers.
unsafe impl Sync for Pipe {}

impl std::fmt::Debug for Pipe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pipe")
            .field("endpoint", &self.endpoint)
            .field("max_packet", &self.max_packet)
            .field("pending", &self.pending.len())
            .finish_non_exhaustive()
    }
}

impl Pipe {
    /// A pipe on `endpoint`, which the caller has registered with `dispatcher`.
    pub(crate) fn new(
        dispatcher: Arc<Dispatcher>,
        interface: Arc<dyn Any + Send + Sync>,
        endpoint: u8,
        kind: u8,
        max_packet: usize,
    ) -> Self {
        Self {
            dispatcher,
            _interface: interface,
            endpoint,
            kind,
            max_packet: max_packet.max(1),
            pending: VecDeque::new(),
        }
    }

    /// Checks an IN transfer length.
    fn check_in_length(&self, len: usize) -> Result<(), Error> {
        if len == 0 || len % self.max_packet != 0 || i32::try_from(len).is_err() {
            return Err(Error::InvalidLength {
                len,
                max_packet: self.max_packet,
            });
        }
        Ok(())
    }

    /// A buffer of `len` bytes, mapped where the platform allows.
    fn alloc(&self, len: usize) -> Buffer {
        self.dispatcher
            .backend()
            .map(len)
            .map_or_else(|| Buffer::heap(len), Buffer::mapped)
    }

    /// Hands `buffer` to the kernel for a transfer of its current length.
    fn submit(&mut self, buffer: Buffer) -> Result<(), Rejected> {
        let Ok(length) = i32::try_from(buffer.len()) else {
            let error = Error::InvalidLength {
                len: buffer.len(),
                max_packet: self.max_packet,
            };
            return Err(Rejected { error, buffer });
        };
        let urb = Urb::new(self.kind, self.endpoint, length);
        let transfer = NonNull::from(Box::leak(Box::new(Transfer { urb, buffer })));
        // The buffer's address is taken once it is in its final place: a pointer taken
        // before the move into the box would not be valid for the kernel's writes.
        // SAFETY: a fresh allocation, referenced nowhere else yet.
        let fresh = unsafe { &mut *transfer.as_ptr() };
        fresh.urb.buffer = fresh.buffer.as_mut_ptr().cast();
        // SAFETY: `transfer` is a fresh allocation that only `pending` refers to from here
        // until the dispatcher hands it back, and nothing reads or writes it meanwhile.
        match unsafe { self.dispatcher.backend().submit(transfer) } {
            Ok(()) => {
                self.pending.push_back(transfer);
                Ok(())
            }
            Err(errno) => {
                // SAFETY: the kernel refused it, so it never took ownership; this is the
                // only reference.
                let Transfer { buffer, .. } = *unsafe { Box::from_raw(transfer.as_ptr()) };
                Err(Rejected {
                    error: errno.into(),
                    buffer,
                })
            }
        }
    }

    /// The oldest transfer once it completes, waiting up to `timeout`.
    fn wait(&mut self, timeout: Duration) -> Result<Option<Completion>, Error> {
        if self.pending.is_empty() {
            return Ok(None);
        }
        let deadline = Instant::now() + timeout;
        let Some(transfer) = self.dispatcher.wait(self.endpoint, deadline)? else {
            return Ok(None);
        };
        Ok(Some(self.take(transfer)))
    }

    /// Removes a transfer the dispatcher handed back from `pending` and unboxes it.
    fn take(&mut self, transfer: NonNull<Transfer>) -> Completion {
        let position = self
            .pending
            .iter()
            .position(|&pending| pending == transfer)
            .expect("invariant: the dispatcher files a transfer only under its own endpoint");
        self.pending.remove(position);
        // SAFETY: reaped, so the kernel is done with it, and it was in `pending`, so it is
        // this pipe's allocation and no other reference remains.
        let Transfer { urb, mut buffer } = *unsafe { Box::from_raw(transfer.as_ptr()) };
        if self.endpoint & 0x80 != 0 {
            let received = usize::try_from(urb.actual_length).unwrap_or(0);
            buffer.set_len(received.min(buffer.capacity()));
        }
        Completion {
            buffer,
            status: TransferError::from_status(urb.status),
        }
    }

    /// Submits nothing more and waits up to `timeout` for the one queued transfer,
    /// cancelling it if it is late.
    fn finish(&mut self, timeout: Duration) -> Result<Completion, Error> {
        if let Some(done) = self.wait(timeout)? {
            return Ok(done);
        }
        self.cancel_all(DROP_GRACE)?;
        Err(Error::TimedOut)
    }

    /// Cancels everything in flight and waits up to `grace` for it all to come back.
    fn cancel_all(&mut self, grace: Duration) -> Result<Vec<Buffer>, Error> {
        for &transfer in &self.pending {
            // SAFETY: submitted and not yet taken back. A transfer that completed meanwhile
            // is refused with EINVAL and still comes back, so the result does not matter.
            let _ = unsafe { self.dispatcher.backend().discard(transfer) };
        }
        let deadline = Instant::now() + grace;
        let mut buffers = Vec::with_capacity(self.pending.len());
        while !self.pending.is_empty() {
            match self.dispatcher.wait(self.endpoint, deadline) {
                Ok(Some(transfer)) => buffers.push(self.take(transfer).buffer),
                Ok(None) => return Err(Error::TimedOut),
                Err(error) => return Err(error),
            }
        }
        Ok(buffers)
    }
}

impl Drop for Pipe {
    fn drop(&mut self) {
        let _ = self.cancel_all(DROP_GRACE);
        if self.pending.is_empty() {
            self.dispatcher.close(self.endpoint);
        }
        // Anything still out is leaked, not freed: the kernel may yet write its outcome. The
        // endpoint stays registered, so no new queue receives those late completions.
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use rustix::io::Errno;

    use super::*;
    use crate::dispatch::fake::Fake;
    use crate::usbfs::URB_BULK;

    const RX: u8 = 0x83;
    const TX: u8 = 0x03;
    const NOTIFY: u8 = 0x82;
    const SOON: Duration = Duration::from_millis(50);

    fn device() -> (Arc<Fake>, Arc<Dispatcher>) {
        let fake = Arc::new(Fake::default());
        let dispatcher = Arc::new(Dispatcher::new(Box::new(Arc::clone(&fake))));
        (fake, dispatcher)
    }

    fn in_queue(dispatcher: &Arc<Dispatcher>, endpoint: u8) -> InQueue {
        dispatcher.open(endpoint).unwrap();
        InQueue::new(Pipe::new(
            Arc::clone(dispatcher),
            Arc::new(()),
            endpoint,
            URB_BULK,
            512,
        ))
    }

    fn out_queue(dispatcher: &Arc<Dispatcher>, endpoint: u8) -> OutQueue {
        dispatcher.open(endpoint).unwrap();
        OutQueue::new(Pipe::new(
            Arc::clone(dispatcher),
            Arc::new(()),
            endpoint,
            URB_BULK,
            512,
        ))
    }

    #[test]
    fn completions_come_back_in_submission_order_with_the_received_length() {
        let (fake, dispatcher) = device();
        let mut rx = in_queue(&dispatcher, RX);
        for _ in 0..3 {
            let buffer = rx.alloc(1024).unwrap();
            rx.submit(buffer).unwrap();
        }
        fake.complete(RX, 0, 1024);
        fake.complete(RX, 0, 512);
        let first = rx
            .wait(SOON)
            .unwrap()
            .expect("the first transfer completed");
        let second = rx
            .wait(SOON)
            .unwrap()
            .expect("the second transfer completed");
        assert_eq!((first.buffer.len(), second.buffer.len()), (1024, 512));
        assert!(
            first.buffer.iter().all(|&byte| byte == RX),
            "the data that arrived is visible"
        );
        assert!(first.status.is_ok() && second.status.is_ok());
        assert!(
            rx.wait(Duration::ZERO).unwrap().is_none(),
            "the third is still in flight"
        );
        assert_eq!(rx.pending(), 1);
    }

    #[test]
    fn a_completion_reaped_by_another_endpoints_waiter_reaches_its_own_queue() {
        let (fake, dispatcher) = device();
        let mut rx = in_queue(&dispatcher, RX);
        let mut notify = in_queue(&dispatcher, NOTIFY);
        let buffer = rx.alloc(512).unwrap();
        rx.submit(buffer).unwrap();
        let buffer = notify.alloc(512).unwrap();
        notify.submit(buffer).unwrap();
        fake.complete(RX, 0, 512);
        // The notification waiter reaps the RX completion and must file it, not drop it.
        assert!(notify.wait(Duration::ZERO).unwrap().is_none());
        assert!(
            rx.wait(Duration::ZERO).unwrap().is_some(),
            "RX's completion was filed"
        );
    }

    #[test]
    fn waiters_on_other_threads_are_woken_by_whichever_thread_reaps() {
        let (fake, dispatcher) = device();
        let mut rx = in_queue(&dispatcher, RX);
        let mut tx = out_queue(&dispatcher, TX);
        let buffer = rx.alloc(512).unwrap();
        rx.submit(buffer).unwrap();
        let mut buffer = tx.alloc(16);
        buffer.set_len(16);
        tx.submit(buffer).unwrap();
        let receiver = thread::spawn(move || rx.wait(Duration::from_secs(10)).unwrap());
        let sender = thread::spawn(move || tx.wait(Duration::from_secs(10)).unwrap());
        fake.complete(TX, 0, 16);
        fake.complete(RX, 0, 512);
        assert!(
            receiver.join().unwrap().is_some(),
            "the RX waiter was woken"
        );
        assert!(sender.join().unwrap().is_some(), "the TX waiter was woken");
    }

    #[test]
    fn dropping_a_queue_cancels_and_reclaims_everything_in_flight() {
        let (fake, dispatcher) = device();
        let mut rx = in_queue(&dispatcher, RX);
        for _ in 0..4 {
            let buffer = rx.alloc(512).unwrap();
            rx.submit(buffer).unwrap();
        }
        drop(rx);
        assert_eq!(fake.discarded(), 4);
        assert_eq!(fake.pending(), 0);
        assert!(
            dispatcher.open(RX).is_ok(),
            "the endpoint is free for a new queue"
        );
    }

    #[test]
    fn a_second_queue_on_the_same_endpoint_is_refused() {
        let (_fake, dispatcher) = device();
        let _rx = in_queue(&dispatcher, RX);
        assert!(matches!(dispatcher.open(RX), Err(Error::Busy)));
    }

    #[test]
    fn in_lengths_must_be_whole_packets() {
        let (_fake, dispatcher) = device();
        let mut rx = in_queue(&dispatcher, RX);
        assert!(matches!(rx.alloc(0), Err(Error::InvalidLength { .. })));
        assert!(matches!(rx.alloc(513), Err(Error::InvalidLength { .. })));
        let rejected = rx.submit(Buffer::heap(100)).unwrap_err();
        assert_eq!(rejected.buffer.capacity(), 100, "the buffer comes back");
    }

    #[test]
    fn unplugging_completes_everything_then_reports_disconnected() {
        let (fake, dispatcher) = device();
        let mut rx = in_queue(&dispatcher, RX);
        let buffer = rx.alloc(512).unwrap();
        rx.submit(buffer).unwrap();
        fake.unplug();
        let done = rx
            .wait(SOON)
            .unwrap()
            .expect("the in-flight transfer comes back");
        assert!(matches!(done.status, Err(TransferError::Disconnected)));
        let buffer = rx.alloc(512).unwrap();
        let rejected = rx.submit(buffer).unwrap_err();
        assert!(matches!(rejected.error, Error::Disconnected));
    }

    #[test]
    fn a_late_read_is_cancelled_and_times_out() {
        let (fake, dispatcher) = device();
        let mut notify = in_queue(&dispatcher, NOTIFY);
        let mut data = [0; 64];
        let read = notify.read(&mut data, Duration::from_millis(10));
        assert!(matches!(read, Err(Error::TimedOut)));
        assert_eq!(fake.pending(), 0, "the late transfer was reclaimed");
        assert_eq!(notify.pending(), 0);
    }

    #[test]
    fn waiting_with_nothing_queued_returns_at_once() {
        let (_fake, dispatcher) = device();
        let mut rx = in_queue(&dispatcher, RX);
        assert!(rx.wait(Duration::from_secs(60)).unwrap().is_none());
    }

    #[test]
    fn a_cancelled_status_is_reported_as_cancelled() {
        let (fake, dispatcher) = device();
        let mut rx = in_queue(&dispatcher, RX);
        let buffer = rx.alloc(512).unwrap();
        rx.submit(buffer).unwrap();
        fake.complete(RX, -Errno::NOENT.raw_os_error(), 0);
        let done = rx.wait(SOON).unwrap().unwrap();
        assert!(matches!(done.status, Err(TransferError::Cancelled)));
        assert!(done.buffer.is_empty());
    }
}
