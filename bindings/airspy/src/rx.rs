use crate::{Airspy, Error, Result, ffi};
use num_complex::Complex;
use std::collections::VecDeque;
use std::ffi::{c_int, c_void};
use std::mem;
use std::slice;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// Fixed sample capacity; about 26 ms at 10 Msps.
const RX_CAPACITY: usize = 262_144;

/// Samples awaiting delivery, plus the size of a gap not yet reported.
struct Buffer {
    samples: VecDeque<Complex<f32>>,
    /// Samples dropped since the queued ones; reported once they are drained.
    dropped: u64,
}

impl Buffer {
    /// Allocate the full capacity up front, so `push` never reallocates.
    fn new() -> Self {
        Self {
            samples: VecDeque::with_capacity(RX_CAPACITY),
            dropped: 0,
        }
    }

    /// Append a callback batch, or count it as dropped if it does not fit.
    ///
    /// Once a batch is dropped, later ones are too until `receive` reports the
    /// gap, so each gap sits at exactly one place in the delivered stream.
    fn push(&mut self, samples: &[Complex<f32>]) {
        if self.dropped > 0 || samples.len() > RX_CAPACITY - self.samples.len() {
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
    fn new() -> Self {
        Self {
            buffer: Mutex::new(Buffer::new()),
            ready: Condvar::new(),
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
    // SAFETY: `ctx` is the RxState start_rx registered; `Airspy::rx_ctx` keeps
    // it alive until stop/close has joined this thread. It is only shared.
    let state = unsafe { &*transfer.ctx.cast::<RxState>() };
    // Only a poisoned lock fails here; a full buffer counts the batch as dropped.
    match state.push(samples) {
        Ok(()) => ffi::RX_CONTINUE,
        Err(_) => ffi::RX_STOP,
    }
}

/// An active reception session, borrowing its device until it stops.
///
/// Dropping it stops reception and discards the status; call
/// [`Receiver::stop`] to see it. The tuning setters stay available while
/// streaming; changing the sample rate needs a stop and a restart.
pub struct Receiver<'a> {
    device: &'a mut Airspy,
    /// This session's buffer; `device.rx_ctx` holds the owning reference.
    state: Arc<RxState>,
}

impl Airspy {
    /// Start float IQ reception with a fresh 262,144-sample private buffer.
    ///
    /// # Errors
    ///
    /// libairspy's status if a forgotten session fails to stop, the sample
    /// type is rejected, or streaming fails to start. A failed start is
    /// stopped again on a best-effort basis.
    pub fn start_rx(&mut self) -> Result<Receiver<'_>> {
        // Only a `mem::forget`-ed Receiver leaves a session running.
        if self.rx_ctx.is_some() {
            self.stop_session()?;
        }
        Error::check(ffi::airspy_set_sample_type(
            self.handle(),
            ffi::SampleType::Float32Iq,
        ))?;
        let state = Arc::new(RxState::new());
        // An Arc, because moving a Box asserts unique access and would
        // invalidate the pointer libairspy's threads read through.
        let ctx = Arc::as_ptr(&state).cast_mut().cast::<c_void>();
        // Stored before starting: callbacks may run before start_rx returns.
        self.rx_ctx = Some(Arc::clone(&state));
        // SAFETY: rx_cb only reads `ctx` as `&RxState`, and `rx_ctx` keeps that
        // RxState alive until stop_session or close has joined the callback
        // threads.
        if let Err(error) = Error::check(unsafe { ffi::airspy_start_rx(self.handle(), rx_cb, ctx) })
        {
            // Join threads created before a partial startup failure.
            let _ = self.stop_session();
            return Err(error);
        }
        Ok(Receiver {
            device: self,
            state,
        })
    }

    /// Stop reception and join callbacks before releasing buffered samples.
    fn stop_session(&mut self) -> Result<()> {
        let result = Error::check(ffi::airspy_stop_rx(self.handle()));
        // airspy_stop_rx joins the callback threads even when it reports
        // failure, so nothing can still read the state released here. The one
        // exception is libairspy's own: see `ffi::airspy_stop_rx`.
        self.rx_ctx.take();
        result
    }
}

impl Receiver<'_> {
    /// Receive up to `samples.len()` IQ pairs, returning the number written.
    ///
    /// The unused tail is untouched. An empty output is invalid. Zero timeout
    /// polls buffered samples; otherwise `timeout` sets the empty-buffer wait
    /// deadline. Mutex contention and copying are not bounded by this timeout.
    /// If the buffer filled, the samples queued before the gap arrive first,
    /// then one `Overflow` reports how many were dropped, then streaming
    /// resumes.
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

    /// Stop reception, join the callback threads and release the device.
    ///
    /// # Errors
    ///
    /// libairspy's status; the session is released either way.
    pub fn stop(self) -> Result<()> {
        // Drop then finds no session and does nothing.
        self.device.stop_session()
    }

    /// Check whether libairspy is still streaming; false after a USB failure
    /// ended the stream.
    pub fn is_streaming(&mut self) -> bool {
        ffi::airspy_is_streaming(self.handle()) == ffi::AIRSPY_TRUE
    }

    /// Borrow the device's live handle, for the shared tuning setters.
    pub(crate) fn handle(&mut self) -> &mut ffi::AirspyDevice {
        self.device.handle()
    }
}

/// Stop the session if `stop` did not.
impl Drop for Receiver<'_> {
    fn drop(&mut self) {
        if self.device.rx_ctx.is_some() {
            let _ = self.device.stop_session();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr;
    use std::thread;
    use std::time::Instant;

    /// Generous wait for cross-thread tests; they assert they finish far sooner.
    const TIMEOUT: Duration = Duration::from_secs(5);

    #[test]
    fn callback_copies_samples_and_preserves_remainder() {
        let state = RxState::new();
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
        let state = RxState::new();
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
        let state = RxState::new();
        // Built up front so the producer only counts the drop (Miri is slow).
        let full = vec![Complex::default(); RX_CAPACITY + 1];
        thread::scope(|scope| {
            scope.spawn(|| state.push(&full).unwrap());
            let mut output = [Complex::default()];
            let start = Instant::now();
            let gap = Error::Overflow {
                dropped: RX_CAPACITY as u64 + 1,
            };
            assert_eq!(state.receive(&mut output, TIMEOUT), Err(gap));
            // Without the wake-up, receive only returns at its deadline.
            assert!(start.elapsed() < Duration::from_secs(4));
        });
    }

    #[test]
    fn empty_receive_times_out_without_modifying_output() {
        let state = RxState::new();
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
        let state = RxState::new();
        let before = Complex::new(1.0_f32, 0.0);
        state.push(&vec![before; RX_CAPACITY]).unwrap();
        state.push(&[Complex::default(); 3]).unwrap();
        let mut output = vec![Complex::default(); RX_CAPACITY];
        assert_eq!(state.receive(&mut output, Duration::ZERO), Ok(RX_CAPACITY));
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
    fn receive_accepts_samples_from_callback_thread() {
        let state = RxState::new();
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
        let state = RxState::new();
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
    fn stop_releases_state_and_allows_restart() {
        let mut device = Airspy::open().unwrap();
        let mut samples = [Complex::default(); 1024];
        for _ in 0..2 {
            let mut rx = device.start_rx().unwrap();
            assert!(rx.receive(&mut samples, TIMEOUT).unwrap() > 0);
            rx.set_frequency(100_000_000).unwrap();
            assert!(rx.receive(&mut samples, TIMEOUT).unwrap() > 0);
            rx.stop().unwrap();
        }
        // A forgotten receiver's session ends when the next one starts.
        std::mem::forget(device.start_rx().unwrap());
        let mut rx = device.start_rx().unwrap();
        assert!(rx.receive(&mut samples, TIMEOUT).unwrap() > 0);
    }
}
