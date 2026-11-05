use crate::device::tuning_setters;
use crate::{Airspy, Error, Result, ffi};
use num_complex::Complex;
use std::collections::VecDeque;
use std::ffi::{c_int, c_void};
use std::fmt;
use std::mem;
use std::slice;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use thiserror::Error as ThisError;

/// Receive capacity used by the crate example, in samples; about 26 ms at
/// 10 Msps.
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

    /// Append a callback batch, or count it as dropped if it does not fit.
    ///
    /// Once a batch is dropped, later ones are too until `receive` reports the
    /// gap, so each gap sits at exactly one place in the delivered stream.
    fn push(&mut self, samples: &[Complex<f32>]) {
        if self.dropped > 0 || samples.len() > self.capacity - self.samples.len() {
            self.dropped = self.dropped.saturating_add(samples.len() as u64);
            return;
        }
        self.samples.extend(samples.iter().copied());
    }
}

/// Stable callback state, owned until libairspy joins its threads.
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
            .wait_timeout_while(buffer, timeout, |buffer| {
                buffer.samples.is_empty() && buffer.dropped == 0
            })
            .map_err(|_| Error::Thread)?;
        let count = samples.len().min(buffer.samples.len());
        if count > 0 {
            for (out, sample) in samples.iter_mut().zip(buffer.samples.drain(..count)) {
                *out = sample;
            }
            return Ok(count);
        }
        // Queued samples come first, so the gap is reported where it happened.
        match mem::take(&mut buffer.dropped) {
            0 => Err(Error::Timeout),
            dropped => Err(Error::Overflow { dropped }),
        }
    }

    /// Drop the queued samples and any unreported gap, returning their total.
    fn discard(&self) -> Result<u64> {
        let mut buffer = self.buffer.lock().map_err(|_| Error::Thread)?;
        let discarded = buffer.samples.len() as u64 + mem::take(&mut buffer.dropped);
        buffer.samples.clear();
        Ok(discarded)
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
    let count = match usize::try_from(transfer.sample_count) {
        Ok(0) | Err(_) => return ffi::RX_CONTINUE,
        Ok(count) => count,
    };
    if transfer.samples.is_null() || transfer.ctx.is_null() {
        return ffi::RX_STOP;
    }
    // SAFETY: start_rx selected float IQ, so `samples` holds `count`
    // interleaved f32 pairs, which is the layout of `Complex<f32>`
    // (`repr(C)`); libairspy keeps them valid until this call returns.
    let samples = unsafe { slice::from_raw_parts(transfer.samples.cast::<Complex<f32>>(), count) };
    // SAFETY: `ctx` is the RxState start_rx registered. Its Receiver keeps it
    // alive until stop/close has joined this thread, and a forgotten Receiver
    // leaks it. It is only shared.
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
#[must_use = "dropping the Receiver stops reception and closes the device"]
pub struct Receiver {
    /// Declared first: dropping the Receiver closes the device, which stops
    /// and joins the callback threads, before `state` is freed.
    device: Airspy,
    /// This session's buffer, owned here alone. It is released only after
    /// stop or Drop has joined the callback threads; `mem::forget` leaks it,
    /// which keeps the still-running callbacks sound.
    state: Arc<RxState>,
}

impl fmt::Debug for Receiver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Receiver").finish_non_exhaustive()
    }
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
    /// The device comes back inside [`StartError`] with
    /// [`Error::InvalidParameter`] for a zero capacity, or libairspy's status
    /// if the sample type is rejected or streaming fails to start. A failed
    /// start is stopped again on a best-effort basis; see
    /// `ffi::airspy_start_rx` for the libairspy 1.0.6 to 1.0.10 crash this
    /// cannot prevent.
    pub fn start_rx(mut self, capacity: usize) -> std::result::Result<Receiver, StartError> {
        match self.start_session(capacity) {
            Ok(state) => Ok(Receiver {
                device: self,
                state,
            }),
            Err(error) => Err(StartError {
                device: self,
                error,
            }),
        }
    }

    /// Allocate the buffer, select float IQ and start libairspy's threads.
    fn start_session(&mut self, capacity: usize) -> Result<Arc<RxState>> {
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
        // still held, and a Receiver releases it only after joining them.
        if let Err(error) = Error::check(unsafe { ffi::airspy_start_rx(self.handle(), rx_cb, ctx) })
        {
            // Join threads created before a partial startup failure.
            let _ = Error::check(ffi::airspy_stop_rx(self.handle()));
            return Err(error);
        }
        Ok(state)
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
    /// short.
    /// A native stop without notification is observed as a timeout after draining
    /// queued samples. This reports adapter overflow, not USB-level sample loss.
    ///
    /// # Errors
    ///
    /// - [`Error::InvalidParameter`] if `samples` is empty.
    /// - [`Error::Timeout`] if nothing arrived within `timeout`.
    /// - [`Error::Overflow`] once per gap, after the samples queued before it.
    /// - [`Error::Thread`] if the buffer lock is poisoned.
    pub fn receive(&mut self, samples: &mut [Complex<f32>], timeout: Duration) -> Result<usize> {
        self.state.receive(samples, timeout)
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
        self.state.discard()
    }

    /// Stop reception, join the callback threads and hand the device back.
    ///
    /// # Errors
    ///
    /// libairspy's status. The device is then closed rather than returned,
    /// because its streaming state is unknown after a failed stop.
    pub fn stop(self) -> Result<Airspy> {
        // Destructured, so a successful stop returns the device instead of
        // dropping, and thereby closing, it.
        let Self { mut device, state } = self;
        let stopped = Error::check(ffi::airspy_stop_rx(device.handle()));
        // airspy_stop_rx joins the callback threads whatever it reports (with
        // the caveats on `ffi::airspy_stop_rx`), so nothing reads `state` now.
        drop(state);
        stopped.map(|()| device)
    }

    /// Check whether libairspy is still streaming.
    ///
    /// False after a USB failure ended the stream. After the callback asked to
    /// stop (only on a poisoned lock), releases 1.0.6 to 1.0.10 still report
    /// true until `stop`; libairspy's master reports false.
    pub fn is_streaming(&mut self) -> bool {
        ffi::airspy_is_streaming(self.handle()) == ffi::AIRSPY_TRUE
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
    use std::ptr;
    use std::thread;
    use std::time::Instant;

    /// Generous wait for cross-thread tests; they assert they finish far sooner.
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
        let mut input = [Complex::new(1.0_f32, -2.0), Complex::new(3.0, -4.0)];
        let mut transfer = ffi::airspy_transfer_t {
            device: ptr::null_mut(),
            ctx: ptr::from_ref(&state).cast_mut().cast(),
            samples: input.as_ptr().cast(),
            sample_count: 2,
        };
        // SAFETY: `transfer`, `input` and `state` outlive the call.
        assert_eq!(unsafe { rx_cb(&raw mut transfer) }, ffi::RX_CONTINUE);
        input[0].re = 99.0;
        let mut first = [Complex::default(); 1];
        assert_eq!(state.receive(&mut first, Duration::ZERO), Ok(1));
        assert_eq!(first[0], Complex::new(1.0_f32, -2.0));
        // The overwrite after the callback did not reach the copy.
        assert_ne!(first[0], input[0]);
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
        transfer.sample_count = 1; // with null samples
        // SAFETY: as above; rx_cb rejects the null samples before reading.
        assert_eq!(unsafe { rx_cb(&raw mut transfer) }, ffi::RX_STOP);
        transfer.ctx = ptr::null_mut();
        // SAFETY: as above; rx_cb rejects the null ctx before reading.
        assert_eq!(unsafe { rx_cb(&raw mut transfer) }, ffi::RX_STOP);
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
            assert!(start.elapsed() < Duration::from_secs(4));
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
            assert!(start.elapsed() < Duration::from_secs(4));
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
            rx.set_frequency(rfus::FrequencyHz::new(100_000_000))
                .unwrap();
            assert!(rx.receive(&mut samples, TIMEOUT).unwrap() > 0);
            device = rx.stop().unwrap();
        }
        // A rejected start hands the device back for another attempt.
        let failed = device.start_rx(0).unwrap_err();
        assert_eq!(failed.error, Error::InvalidParameter);
        let mut rx = failed.device.start_rx(DEFAULT_RX_CAPACITY).unwrap();
        assert!(rx.receive(&mut samples, TIMEOUT).unwrap() > 0);
    }
}
