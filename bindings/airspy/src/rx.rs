use crate::device::tuning_setters;
use crate::{Airspy, Error, LibVersion, Result, ffi};
use num_complex::Complex;
use std::collections::VecDeque;
use std::ffi::{c_int, c_void};
use std::mem;
use std::slice;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use thiserror::Error as ThisError;

/// A capacity for a reader that drains continuously: 262,144 samples (2 MiB),
/// about 26 ms at 10 Msps.
pub const DEFAULT_RX_CAPACITY: usize = 262_144;

/// Samples awaiting delivery, plus the size of a gap not yet reported.
struct Buffer {
    samples: VecDeque<Complex<f32>>,
    /// Most samples ever queued; `push` never grows past it.
    capacity: usize,
    /// Samples dropped since the queued ones; reported once they are drained.
    dropped: u64,
}

impl Buffer {
    /// Allocate the full capacity up front, so `push` never reallocates.
    fn new(capacity: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(capacity),
            capacity,
            dropped: 0,
        }
    }

    /// Whether there is nothing to deliver: no samples and no gap.
    fn is_idle(&self) -> bool {
        self.samples.is_empty() && self.dropped == 0
    }

    /// Append a callback batch, or count it as dropped if it does not fit.
    ///
    /// Once a batch is dropped, later ones are too until `drain_into` reports
    /// the gap, so each gap sits at exactly one place in the delivered stream.
    fn push(&mut self, samples: &[Complex<f32>]) {
        if self.dropped > 0 || samples.len() > self.capacity - self.samples.len() {
            self.dropped = self.dropped.saturating_add(samples.len() as u64);
            return;
        }
        self.samples.extend(samples.iter().copied());
    }

    /// Move up to `out.len()` queued samples into `out`; once none are queued,
    /// report the gap, if any, or a timeout.
    fn drain_into(&mut self, out: &mut [Complex<f32>]) -> Result<usize> {
        let count = out.len().min(self.samples.len());
        if count > 0 {
            // Two slice copies, since the ring may wrap; draining a prefix of
            // `Copy` samples then only moves the head.
            let (front, back) = self.samples.as_slices();
            let split = count.min(front.len());
            out[..split].copy_from_slice(&front[..split]);
            out[split..count].copy_from_slice(&back[..count - split]);
            self.samples.drain(..count);
            return Ok(count);
        }
        // Queued samples come first, so the gap is reported where it happened.
        match mem::take(&mut self.dropped) {
            0 => Err(Error::Timeout),
            dropped => Err(Error::Overflow { dropped }),
        }
    }

    /// Drop the queued samples and any unreported gap, returning their total.
    fn clear(&mut self) -> u64 {
        let cleared = self.samples.len() as u64 + mem::take(&mut self.dropped);
        self.samples.clear();
        cleared
    }
}

/// Stable callback state, owned until libairspy joins its threads, or leaked
/// when the linked libairspy may not join them.
pub(crate) struct RxState {
    buffer: Mutex<Buffer>,
    ready: Condvar,
}

impl RxState {
    /// Allocate the bounded sample storage before starting callbacks.
    ///
    /// A zero capacity could never deliver a sample, so it is
    /// [`Error::InvalidParameter`].
    fn new(capacity: usize) -> Result<Self> {
        if capacity == 0 {
            return Err(Error::InvalidParameter);
        }
        Ok(Self {
            buffer: Mutex::new(Buffer::new(capacity)),
            ready: Condvar::new(),
        })
    }

    /// Leak one reference before a stop that may return without joining the
    /// callback threads, so they never read freed state. Only a small shell
    /// leaks: the sample storage is freed, and a late callback finds no room
    /// and drops its batch without allocating.
    fn leak_in_case_stop_skips_join(self: &Arc<Self>) {
        mem::forget(Arc::clone(self));
        if let Ok(mut buffer) = self.buffer.lock() {
            *buffer = Buffer::new(0);
        }
    }

    /// Copy a callback batch without growing storage or waiting for free capacity.
    fn push(&self, samples: &[Complex<f32>]) -> Result<()> {
        let result = self
            .buffer
            .lock()
            .map(|mut buffer| buffer.push(samples))
            .map_err(|_| Error::Thread);
        // Wake a waiting receive for samples and for a gap alike.
        self.ready.notify_one();
        result
    }

    /// Copy available samples, waiting up to `timeout` when empty.
    fn receive(&self, samples: &mut [Complex<f32>], timeout: Duration) -> Result<usize> {
        if samples.is_empty() {
            return Err(Error::InvalidParameter);
        }
        let buffer = self.buffer.lock().map_err(|_| Error::Thread)?;
        let (mut buffer, _) = self
            .ready
            .wait_timeout_while(buffer, timeout, |buffer| buffer.is_idle())
            .map_err(|_| Error::Thread)?;
        buffer.drain_into(samples)
    }

    /// Drop the queued samples and any unreported gap, returning their total.
    fn discard(&self) -> Result<u64> {
        let mut buffer = self.buffer.lock().map_err(|_| Error::Thread)?;
        Ok(buffer.clear())
    }
}

/// Copy libairspy's borrowed samples into the private receive buffer.
///
/// # Safety
///
/// `transfer` is null or libairspy's transfer for this call, whose `ctx` is
/// null or the `RxState` that `start_rx` registered and keeps alive.
unsafe extern "C" fn rx_cb(transfer: *mut ffi::airspy_transfer_t) -> c_int {
    // SAFETY: per this function's contract, libairspy owns the transfer and
    // keeps it valid for the duration of the call.
    let Some(transfer) = (unsafe { transfer.as_ref() }) else {
        return ffi::RX_STOP;
    };
    let Ok(count @ 1..) = usize::try_from(transfer.sample_count) else {
        return ffi::RX_CONTINUE;
    };
    if transfer.samples.is_null() || transfer.ctx.is_null() {
        return ffi::RX_STOP;
    }
    // SAFETY: start_rx selected float IQ, so `samples` holds `count`
    // interleaved f32 pairs, which is the layout of `Complex<f32>`
    // (`repr(C)`); libairspy keeps them valid until this call returns.
    let samples = unsafe { slice::from_raw_parts(transfer.samples.cast::<Complex<f32>>(), count) };
    // SAFETY: `ctx` is the RxState start_rx registered. The device's session
    // keeps it alive until a stop has joined this thread; where the stop may
    // not join, and when a Receiver is forgotten, it is leaked. It is only
    // shared.
    let state = unsafe { &*transfer.ctx.cast::<RxState>() };
    // Only a poisoned lock fails here; a full buffer counts the batch as dropped.
    match state.push(samples) {
        Ok(()) => ffi::RX_CONTINUE,
        Err(_) => ffi::RX_STOP,
    }
}

/// An active reception session, owning its device until it stops.
///
/// [`Receiver::stop`] hands the device back; dropping the Receiver instead
/// closes the device and discards the stop status. The tuning setters stay
/// available while streaming; changing the sample rate needs a stop and a
/// restart.
#[derive(Debug)]
#[must_use = "dropping the Receiver stops reception and closes the device"]
pub struct Receiver {
    /// The device, whose session holds this reception's buffer. Dropping it
    /// ends the session before the buffer is released.
    device: Airspy,
}

/// A failed [`Airspy::start_rx`], handing the device back for another attempt.
#[derive(Debug, ThisError)]
#[error("reception did not start")]
pub struct StartError {
    /// The device, still open and not streaming.
    pub device: Airspy,
    /// Why no session started.
    #[source]
    pub error: Error,
}

impl Airspy {
    /// Start float IQ reception into a private buffer of `capacity` samples.
    ///
    /// The buffer is allocated up front, eight bytes per sample. Size it for
    /// the longest pause between [`Receiver::receive`] calls, which documents
    /// what happens once it fills.
    ///
    /// # Errors
    ///
    /// The device comes back inside [`StartError`] with:
    /// - [`Error::InvalidParameter`] for a zero capacity;
    /// - libairspy's status if the sample type is rejected or streaming fails
    ///   to start. On libairspy 1.0.6 to 1.0.10, a start that fails partway
    ///   can crash inside libairspy when it is stopped; no caller can prevent
    ///   it;
    /// - [`Error::StreamingStopped`] if, on libairspy before 1.0.11, an
    ///   earlier stop found the stream already ended by a USB failure.
    ///   libairspy's threads may still be running, so the device cannot
    ///   receive again in this process, and dropping it leaks its handle.
    ///   Best-effort: a USB failure that races the stop goes undetected, and
    ///   restarting or dropping the device is then undefined behaviour inside
    ///   libairspy.
    pub fn start_rx(mut self, capacity: usize) -> std::result::Result<Receiver, StartError> {
        match self.start_session(capacity) {
            Ok(()) => Ok(Receiver { device: self }),
            Err(error) => Err(StartError {
                device: self,
                error,
            }),
        }
    }

    /// Allocate the buffer, select float IQ and start libairspy's threads.
    fn start_session(&mut self, capacity: usize) -> Result<()> {
        if self.threads_orphaned {
            return Err(Error::StreamingStopped);
        }
        // Validated before the device is touched, so a rejected capacity
        // changes nothing.
        let state = Arc::new(RxState::new(capacity)?);
        Error::check(ffi::airspy_set_sample_type(
            self.handle(),
            ffi::SampleType::Float32Iq,
        ))?;
        // An Arc, because moving a Box asserts unique access and would
        // invalidate the pointer libairspy's threads read through.
        let ctx = Arc::as_ptr(&state).cast_mut().cast::<c_void>();
        // SAFETY: rx_cb only reads `ctx` as `&RxState`. `state` outlives the
        // callback threads: a failed start joins them below while `state` is
        // still held, and `end_session` joins them before releasing it, or
        // leaks it where the stop may not join.
        if let Err(error) = Error::check(unsafe { ffi::airspy_start_rx(self.handle(), rx_cb, ctx) })
        {
            // Join threads created before a partial startup failure. A failed
            // start leaves `streaming` set whenever it created threads, so this
            // stop joins them, and it never orphans the device.
            let _ = ffi::airspy_stop_rx(self.handle());
            return Err(error);
        }
        self.session = Some(state);
        Ok(())
    }

    /// End the running session, if any, and release its state.
    ///
    /// Before 1.0.11 the stop may skip the join (see `ffi::airspy_stop_rx`),
    /// so the state is leaked, and a stream already ended marks the threads
    /// orphaned. Best-effort: a failure after the check slips past it.
    pub(crate) fn end_session(&mut self) -> Result<()> {
        let Some(state) = self.session.take() else {
            return Ok(());
        };
        if !LibVersion::linked().stop_always_joins() {
            if !self.is_streaming() {
                self.threads_orphaned = true;
            }
            state.leak_in_case_stop_skips_join();
        }
        let stopped = Error::check(ffi::airspy_stop_rx(self.handle()));
        // Released only now that the stop has joined the threads reading it.
        drop(state);
        stopped
    }

    /// Whether libairspy reports this device streaming.
    fn is_streaming(&mut self) -> bool {
        ffi::airspy_is_streaming(self.handle()) == ffi::AIRSPY_TRUE
    }
}

impl Receiver {
    /// Receive up to `samples.len()` IQ pairs, returning the number written.
    ///
    /// The unused tail is untouched. An empty output is invalid. Zero timeout
    /// polls buffered samples; otherwise `timeout` sets the empty-buffer wait
    /// deadline. Mutex contention and copying are not bounded by this timeout.
    /// If the buffer filled, the samples queued before the gap arrive first,
    /// then one `Overflow` reports how many were dropped, then streaming
    /// resumes. Batches arriving while that backlog drains are dropped too, so
    /// the gap lasts until the backlog is read; draining promptly keeps it
    /// short. `Overflow` reports adapter overflow, not USB-level sample loss.
    ///
    /// Once libairspy has ended the stream itself (a USB failure), queued
    /// samples still drain first.
    ///
    /// # Errors
    ///
    /// - [`Error::InvalidParameter`] if `samples` is empty.
    /// - [`Error::Timeout`] if nothing arrived within `timeout` while streaming.
    /// - [`Error::StreamingStopped`] if nothing is queued and libairspy has
    ///   ended the stream; call [`Receiver::stop`], then start again (see
    ///   [`Airspy::start_rx`] for libairspy before 1.0.11).
    /// - [`Error::Overflow`] once per gap, after the samples queued before it.
    /// - [`Error::Thread`] if the buffer lock is poisoned.
    pub fn receive(&mut self, samples: &mut [Complex<f32>], timeout: Duration) -> Result<usize> {
        match self.state().receive(samples, timeout) {
            // Poll once more for a final batch that landed before the check.
            Err(Error::Timeout) if !self.is_streaming() => {
                match self.state().receive(samples, Duration::ZERO) {
                    Err(Error::Timeout) => Err(Error::StreamingStopped),
                    result => result,
                }
            }
            result => result,
        }
    }

    /// Drop every queued sample and any unreported gap, for example after a
    /// retune, so the next `receive` starts from samples captured after this
    /// call. Returns how many samples went: the queued ones plus the gap.
    /// Batches still in flight when it returns are kept.
    ///
    /// # Errors
    ///
    /// [`Error::Thread`] if the buffer lock is poisoned.
    pub fn discard(&mut self) -> Result<u64> {
        self.state().discard()
    }

    /// Stop reception, join the callback threads and hand the device back.
    ///
    /// After a USB failure on libairspy before 1.0.11 the device comes back
    /// unable to start again; see [`Airspy::start_rx`].
    ///
    /// # Errors
    ///
    /// libairspy's status. The device is then closed rather than returned,
    /// because its streaming state is unknown after a failed stop.
    pub fn stop(self) -> Result<Airspy> {
        let mut device = self.device;
        device.end_session().map(|()| device)
    }

    /// Check whether libairspy is still streaming.
    ///
    /// False after a USB failure ended the stream. After the callback asked to
    /// stop (only on a poisoned lock), releases 1.0.6 to 1.0.10 still report
    /// true until `stop`; 1.0.11 and later report false.
    pub fn is_streaming(&mut self) -> bool {
        self.device.is_streaming()
    }

    /// This reception's buffer: `start_rx` set the device's session before
    /// the Receiver existed, and only `stop` or `Drop` end it.
    fn state(&self) -> &RxState {
        self.device
            .session
            .as_deref()
            .expect("invariant: a Receiver's device has a running session")
    }

    /// Borrow the device's live handle, for the shared tuning setters.
    pub(crate) fn handle(&mut self) -> &mut ffi::AirspyDevice {
        self.device.handle()
    }
}

/// Tuning while streaming, identical to [`Airspy`]'s.
#[expect(
    clippy::missing_errors_doc,
    reason = "each returns libairspy's status unchanged; Error documents the variants"
)]
impl Receiver {
    tuning_setters!();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FrequencyHz;
    use std::ptr;
    use std::thread;
    use std::time::Instant;

    /// Generous wait for cross-thread tests; a missed wake-up takes all of it.
    const TIMEOUT: Duration = Duration::from_secs(5);

    /// Small enough that the overflow tests stay quick under Miri.
    const CAPACITY: usize = 8;

    fn state() -> RxState {
        RxState::new(CAPACITY).unwrap()
    }

    #[test]
    fn receiver_moves_between_threads() {
        fn assert_send<T: Send>() {}
        assert_send::<Receiver>();
        assert_send::<StartError>();
    }

    #[test]
    fn zero_capacity_is_rejected() {
        assert!(matches!(RxState::new(0), Err(Error::InvalidParameter)));
    }

    #[test]
    fn callback_copies_samples_and_preserves_remainder() {
        let state = state();
        // airspy.h's float IQ layout: interleaved I, Q pairs of f32.
        let mut input = [1.0_f32, -2.0, 3.0, -4.0];
        let mut transfer = ffi::airspy_transfer_t {
            device: ptr::null_mut(),
            ctx: ptr::from_ref(&state).cast_mut().cast(),
            samples: input.as_ptr().cast(),
            sample_count: 2,
        };
        // SAFETY: `transfer`, `input` and `state` outlive the call.
        assert_eq!(unsafe { rx_cb(&raw mut transfer) }, ffi::RX_CONTINUE);
        input[0] = 99.0;
        let mut first = [Complex::default(); 1];
        assert_eq!(state.receive(&mut first, Duration::ZERO), Ok(1));
        // The overwrite after the callback did not reach the copy.
        assert_eq!(first[0], Complex::new(1.0_f32, -2.0));
        assert_ne!(first[0].re, input[0]);
        let mut remaining = [Complex::new(9.0, 9.0); 3];
        assert_eq!(state.receive(&mut remaining, Duration::ZERO), Ok(1));
        assert_eq!(
            remaining,
            [
                Complex::new(3.0_f32, -4.0),
                Complex::new(9.0, 9.0),
                Complex::new(9.0, 9.0)
            ]
        );
    }

    #[test]
    fn callback_guards_stop_or_skip_malformed_transfers() {
        // SAFETY: a null transfer is within rx_cb's contract.
        assert_eq!(unsafe { rx_cb(ptr::null_mut()) }, ffi::RX_STOP);
        let state = state();
        let mut transfer = ffi::airspy_transfer_t {
            device: ptr::null_mut(),
            ctx: ptr::from_ref(&state).cast_mut().cast(),
            samples: ptr::null(),
            sample_count: 0,
        };
        // SAFETY: `transfer` and `state` outlive the call.
        assert_eq!(unsafe { rx_cb(&raw mut transfer) }, ffi::RX_CONTINUE);
        transfer.sample_count = -1;
        // SAFETY: as above; rx_cb skips a negative count before reading.
        assert_eq!(unsafe { rx_cb(&raw mut transfer) }, ffi::RX_CONTINUE);
        transfer.sample_count = 1; // with null samples
        // SAFETY: as above; rx_cb rejects the null samples before reading.
        assert_eq!(unsafe { rx_cb(&raw mut transfer) }, ffi::RX_STOP);
        let input = [0.0_f32; 2];
        transfer.samples = input.as_ptr().cast();
        transfer.ctx = ptr::null_mut(); // with valid samples
        // SAFETY: as above; rx_cb rejects the null ctx before reading it.
        assert_eq!(unsafe { rx_cb(&raw mut transfer) }, ffi::RX_STOP);
        assert_eq!(
            state.receive(&mut [Complex::default()], Duration::ZERO),
            Err(Error::Timeout)
        );
    }

    #[test]
    fn overflow_wakes_a_waiting_receive() {
        let state = state();
        // Built up front so the producer only counts the drop.
        let full = vec![Complex::default(); CAPACITY + 1];
        thread::scope(|scope| {
            scope.spawn(|| state.push(&full).unwrap());
            let mut output = [Complex::default()];
            let start = Instant::now();
            let gap = Error::Overflow {
                dropped: CAPACITY as u64 + 1,
            };
            assert_eq!(state.receive(&mut output, TIMEOUT), Err(gap));
            // Without the wake-up, receive only returns at its deadline.
            assert!(start.elapsed() < TIMEOUT);
        });
    }

    #[test]
    fn overflow_waits_until_the_whole_backlog_is_read() {
        let state = state();
        state.push(&[Complex::default(); CAPACITY]).unwrap();
        state.push(&[Complex::default()]).unwrap();
        let mut half = vec![Complex::default(); CAPACITY / 2];
        assert_eq!(state.receive(&mut half, Duration::ZERO), Ok(CAPACITY / 2));
        assert_eq!(state.receive(&mut half, Duration::ZERO), Ok(CAPACITY / 2));
        assert_eq!(
            state.receive(&mut half, Duration::ZERO),
            Err(Error::Overflow { dropped: 1 })
        );
    }

    #[test]
    fn empty_receive_times_out_without_modifying_output() {
        let state = state();
        let mut output = [Complex::new(9.0, 9.0)];
        assert_eq!(
            state.receive(&mut output, Duration::ZERO),
            Err(Error::Timeout)
        );
        assert_eq!(output[0], Complex::new(9.0_f32, 9.0));
        assert_eq!(
            state.receive(&mut [], Duration::ZERO),
            Err(Error::InvalidParameter)
        );
    }

    #[test]
    fn overflow_reports_the_gap_after_queued_samples_then_resumes() {
        let state = state();
        let before = Complex::new(1.0_f32, 0.0);
        state.push(&[before; CAPACITY]).unwrap();
        state.push(&[Complex::default(); 3]).unwrap();
        let mut output = vec![Complex::default(); CAPACITY];
        assert_eq!(state.receive(&mut output, Duration::ZERO), Ok(CAPACITY));
        assert!(output.iter().all(|sample| *sample == before));
        // Still inside the gap: dropped even though there is room now.
        state.push(&[Complex::default()]).unwrap();
        assert_eq!(
            state.receive(&mut output, Duration::ZERO),
            Err(Error::Overflow { dropped: 4 })
        );
        let after = Complex::new(2.0_f32, 0.0);
        state.push(&[after]).unwrap();
        assert_eq!(state.receive(&mut output, Duration::ZERO), Ok(1));
        assert_eq!(output[0], after);
    }

    #[test]
    fn receive_reads_in_order_across_the_ring_wrap() {
        let state = state();
        let sample = |i: u8| Complex::new(f32::from(i), 0.0);
        let first: Vec<_> = (0..6).map(sample).collect();
        let second: Vec<_> = (6..12).map(sample).collect();
        state.push(&first).unwrap();
        let mut output = [Complex::default(); CAPACITY];
        assert_eq!(state.receive(&mut output[..4], Duration::ZERO), Ok(4));
        // Eight queued samples now wrap past the end of the ring's storage.
        state.push(&second).unwrap();
        {
            let buffer = state.buffer.lock().unwrap();
            assert_ne!(buffer.samples.as_slices().1.len(), 0, "the queue wraps");
        }
        assert_eq!(state.receive(&mut output, Duration::ZERO), Ok(CAPACITY));
        let expected: Vec<_> = (4..12).map(sample).collect();
        assert_eq!(output.as_slice(), expected.as_slice());
    }

    #[test]
    fn discard_clears_queued_samples_and_the_pending_gap() {
        let state = state();
        state.push(&[Complex::default(); CAPACITY]).unwrap();
        state.push(&[Complex::default()]).unwrap();
        assert_eq!(state.discard(), Ok(CAPACITY as u64 + 1));
        let mut output = [Complex::default()];
        assert_eq!(
            state.receive(&mut output, Duration::ZERO),
            Err(Error::Timeout)
        );
        // Not inside a gap any more: the next batch is kept.
        let after = Complex::new(2.0_f32, 0.0);
        state.push(&[after]).unwrap();
        assert_eq!(state.receive(&mut output, Duration::ZERO), Ok(1));
        assert_eq!(output[0], after);
        assert_eq!(state.discard(), Ok(0));
    }

    #[test]
    fn receive_accepts_samples_from_callback_thread() {
        let state = state();
        thread::scope(|scope| {
            scope.spawn(|| state.push(&[Complex::new(1.0, 2.0)]).unwrap());
            let mut output = [Complex::default()];
            let start = Instant::now();
            assert_eq!(state.receive(&mut output, TIMEOUT), Ok(1));
            assert!(start.elapsed() < TIMEOUT);
            assert_eq!(output[0], Complex::new(1.0_f32, 2.0));
        });
    }

    #[test]
    fn poisoned_state_stops_without_unwinding() {
        let state = state();
        thread::scope(|scope| {
            let worker = scope.spawn(|| {
                let _guard = state.buffer.lock().unwrap();
                panic!("poison test");
            });
            assert!(worker.join().is_err());
        });
        assert_eq!(state.push(&[Complex::default()]), Err(Error::Thread));
        let input = [0.0_f32; 2];
        let mut transfer = ffi::airspy_transfer_t {
            device: ptr::null_mut(),
            ctx: ptr::from_ref(&state).cast_mut().cast(),
            samples: input.as_ptr().cast(),
            sample_count: 1,
        };
        // SAFETY: `transfer`, `input` and `state` outlive the call.
        assert_eq!(unsafe { rx_cb(&raw mut transfer) }, ffi::RX_STOP);
        assert_eq!(
            state.receive(&mut [Complex::default()], Duration::ZERO),
            Err(Error::Thread)
        );
    }

    #[test]
    #[ignore = "requires an attached Airspy device; run with RUST_TEST_THREADS=1"]
    fn stop_returns_the_device_for_a_restart() {
        let mut device = Airspy::open().unwrap();
        let mut samples = [Complex::default(); 1024];
        for _ in 0..2 {
            let mut rx = device.start_rx(DEFAULT_RX_CAPACITY).unwrap();
            assert!(rx.receive(&mut samples, TIMEOUT).unwrap() > 0);
            rx.set_frequency(FrequencyHz::new(100_000_000)).unwrap();
            assert!(rx.receive(&mut samples, TIMEOUT).unwrap() > 0);
            device = rx.stop().unwrap();
        }
        // A rejected start hands the device back for another attempt.
        let failed = device.start_rx(0).unwrap_err();
        assert_eq!(failed.error, Error::InvalidParameter);
        let mut rx = failed.device.start_rx(DEFAULT_RX_CAPACITY).unwrap();
        assert!(rx.receive(&mut samples, TIMEOUT).unwrap() > 0);
    }

    /// The session lifecycle against `ffi::fake`, whose callback thread reads
    /// the real `ctx` pointer, so Miri sees any read of freed state.
    #[cfg(miri)]
    mod lifecycle {
        use super::*;
        use crate::ffi::fake;
        use std::sync::Weak;

        /// Receive one batch from the fake thread, returning a handle on the
        /// session's state.
        fn receive_some(rx: &mut Receiver) -> Weak<RxState> {
            let mut output = [Complex::default(); 4];
            assert!(rx.receive(&mut output, TIMEOUT).unwrap() > 0);
            assert_eq!(output[0], Complex::new(fake::SAMPLE, fake::SAMPLE));
            Arc::downgrade(rx.device.session.as_ref().expect("a running session"))
        }

        /// Release the one reference `leak_in_case_stop_skips_join` leaked, once
        /// the callback thread is gone, and check the state is then freed.
        fn reclaim(state: &Weak<RxState>) {
            let strong = state.upgrade().expect("state was leaked");
            // SAFETY: exactly one reference was leaked, and `strong` keeps
            // the count above zero while it is released.
            unsafe { Arc::decrement_strong_count(Arc::as_ptr(&strong)) };
            drop(strong);
            assert!(state.upgrade().is_none());
        }

        #[test]
        fn stop_and_drop_free_the_state_after_joining() {
            let device = Airspy::open().unwrap();
            let mut rx = device.start_rx(CAPACITY).unwrap();
            let state = receive_some(&mut rx);
            let device = rx.stop().unwrap();
            assert!(state.upgrade().is_none());
            let mut rx = device.start_rx(CAPACITY).unwrap();
            let state = receive_some(&mut rx);
            drop(rx);
            assert!(state.upgrade().is_none());
        }

        #[test]
        fn old_releases_leak_the_state_a_stop_may_not_join() {
            fake::REVISION.set(10);
            let device = Airspy::open().unwrap();
            let mut rx = device.start_rx(CAPACITY).unwrap();
            let state = receive_some(&mut rx);
            fake::fail_usb(rx.handle());
            // The stop skips the join; the callback thread keeps reading ctx.
            let device = rx.stop().unwrap();
            assert!(state.upgrade().is_some());
            // libairspy's own threads are orphaned too, so the device refuses.
            let failed = device.start_rx(CAPACITY).unwrap_err();
            assert_eq!(failed.error, Error::StreamingStopped);
            // Dropping it leaks the handle rather than closing it.
            drop(failed);
            fake::close_leaked();
            reclaim(&state);
        }

        #[test]
        fn old_releases_leak_the_state_when_a_receiver_drops_unjoined() {
            fake::REVISION.set(10);
            let device = Airspy::open().unwrap();
            let mut rx = device.start_rx(CAPACITY).unwrap();
            let state = receive_some(&mut rx);
            fake::fail_usb(rx.handle());
            drop(rx);
            assert!(state.upgrade().is_some());
            fake::close_leaked();
            reclaim(&state);
        }

        #[test]
        fn old_releases_restart_after_a_healthy_stop() {
            fake::REVISION.set(10);
            let device = Airspy::open().unwrap();
            let mut rx = device.start_rx(CAPACITY).unwrap();
            let first = receive_some(&mut rx);
            let device = rx.stop().unwrap();
            let mut rx = device.start_rx(CAPACITY).unwrap();
            let second = receive_some(&mut rx);
            drop(rx);
            // Joined, so nothing runs on them, but each stop leaked its shell.
            reclaim(&first);
            reclaim(&second);
        }

        #[test]
        fn old_releases_retry_after_a_failed_start() {
            fake::REVISION.set(10);
            fake::FAIL_START.set(true);
            let device = Airspy::open().unwrap();
            let failed = device.start_rx(CAPACITY).unwrap_err();
            assert_eq!(failed.error, Error::LibUsb);
            fake::FAIL_START.set(false);
            let mut rx = failed.device.start_rx(CAPACITY).unwrap();
            let state = receive_some(&mut rx);
            drop(rx);
            reclaim(&state);
        }

        #[test]
        fn receive_reports_a_stream_libairspy_ended() {
            let device = Airspy::open().unwrap();
            let mut rx = device.start_rx(CAPACITY).unwrap();
            receive_some(&mut rx);
            fake::fail_usb(rx.handle());
            fake::end_callbacks(rx.handle());
            let mut output = [Complex::default(); CAPACITY];
            let error = loop {
                match rx.receive(&mut output, Duration::ZERO) {
                    Ok(_) | Err(Error::Overflow { .. }) => {}
                    Err(error) => break error,
                }
            };
            assert_eq!(error, Error::StreamingStopped);
        }

        #[test]
        fn receive_times_out_while_still_streaming() {
            fake::DELIVER.set(false);
            let device = Airspy::open().unwrap();
            let mut rx = device.start_rx(CAPACITY).unwrap();
            assert_eq!(
                rx.receive(&mut [Complex::default()], Duration::ZERO),
                Err(Error::Timeout)
            );
            rx.stop().unwrap();
        }
    }
}
