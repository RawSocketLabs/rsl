use crate::{Airspy, Error, Result, ffi};
use num_complex::Complex;
use std::collections::VecDeque;
use std::ffi::{c_int, c_void};
use std::slice;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// Fixed sample capacity; about 26 ms at 10 Msps.
const RX_CAPACITY: usize = 262_144;

/// Samples awaiting delivery, plus a terminal overflow indication.
struct Buffer {
    samples: VecDeque<Complex<f32>>,
    overflow: bool,
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
            buffer: Mutex::new(Buffer {
                samples: VecDeque::with_capacity(RX_CAPACITY),
                overflow: false,
            }),
            ready: Condvar::new(),
        }
    }

    /// Copy a callback batch without growing storage or waiting for free capacity.
    fn push(&self, samples: &[Complex<f32>]) -> Result<()> {
        let result = match self.buffer.lock() {
            Err(_) => Err(Error::Thread),
            Ok(mut buffer) => {
                if buffer.overflow || samples.len() > RX_CAPACITY - buffer.samples.len() {
                    buffer.overflow = true;
                    Err(Error::Overflow)
                } else {
                    buffer.samples.extend(samples.iter().copied());
                    Ok(())
                }
            }
        };
        self.ready.notify_one();
        result
    }

    /// Copy available samples, waiting up to the timeout when empty.
    fn receive(&self, samples: &mut [Complex<f32>], timeout_ms: u32) -> Result<usize> {
        if samples.is_empty() {
            return Err(Error::InvalidParameter);
        }
        let timeout = Duration::from_millis(timeout_ms.into());
        let buffer = self.buffer.lock().map_err(|_| Error::Thread)?;
        let (mut buffer, _) = self
            .ready
            .wait_timeout_while(buffer, timeout, |buffer| {
                buffer.samples.is_empty() && !buffer.overflow
            })
            .map_err(|_| Error::Thread)?;
        if buffer.overflow {
            return Err(Error::Overflow);
        }
        if buffer.samples.is_empty() {
            return Err(Error::Timeout);
        }
        let count = samples.len().min(buffer.samples.len());
        for (out, sample) in samples.iter_mut().zip(buffer.samples.drain(..count)) {
            *out = sample;
        }
        Ok(count)
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
    if transfer.sample_count <= 0 {
        return ffi::RX_CONTINUE;
    }
    if transfer.samples.is_null() || transfer.ctx.is_null() {
        return ffi::RX_STOP;
    }
    // SAFETY: start_rx selected float IQ, so `samples` holds `sample_count`
    // interleaved f32 pairs, which is the layout of `Complex<f32>`
    // (`repr(C)`); libairspy keeps them valid until this call returns.
    let samples = unsafe {
        slice::from_raw_parts(
            transfer.samples.cast::<Complex<f32>>(),
            transfer.sample_count as usize,
        )
    };
    // SAFETY: `ctx` is the RxState start_rx registered; `Airspy::rx_ctx` keeps
    // it alive until stop/close has joined this thread. It is only shared.
    let state = unsafe { &*transfer.ctx.cast::<RxState>() };
    match state.push(samples) {
        Ok(()) => ffi::RX_CONTINUE,
        Err(_) => ffi::RX_STOP,
    }
}

impl Airspy {
    /// Start float IQ reception with a fresh 262,144-sample private buffer.
    ///
    /// Calling this again synchronously stops the previous session first.
    pub fn start_rx(&mut self) -> Result<()> {
        if self.rx_ctx.is_some() {
            self.stop_rx()?;
        }
        Error::check(ffi::airspy_set_sample_type(
            self.handle(),
            ffi::SampleType::Float32Iq,
        ))?;
        let state = Arc::new(RxState::new());
        // Shared ownership: a Box move would invalidate the pointer
        // libairspy's threads read through.
        let ctx = Arc::as_ptr(&state).cast_mut().cast::<c_void>();
        // Stored before starting: callbacks may run before start_rx returns.
        self.rx_ctx = Some(state);
        // SAFETY: rx_cb only reads `ctx` as `&RxState`, and `rx_ctx` keeps that
        // RxState alive until stop_rx or close has joined the callback threads.
        let result = Error::check(unsafe { ffi::airspy_start_rx(self.handle(), rx_cb, ctx) });
        if result.is_err() {
            // Join threads created before a partial startup failure.
            let _ = self.stop_rx();
        }
        result
    }

    /// Receive up to `samples.len()` IQ pairs, returning the number written.
    ///
    /// The unused tail is untouched. An empty output is invalid. Zero timeout
    /// polls buffered samples; otherwise `timeout_ms` sets the empty-buffer wait
    /// deadline. Mutex contention and copying are not bounded by this timeout.
    /// Overflow takes precedence over queued samples and persists until restart.
    /// A native stop without notification is observed as a timeout after draining
    /// queued samples. This reports adapter overflow, not USB-level sample loss.
    pub fn receive(&mut self, samples: &mut [Complex<f32>], timeout_ms: u32) -> Result<usize> {
        let state = self.rx_ctx.as_ref().ok_or(Error::StreamingStopped)?;
        state.receive(samples, timeout_ms)
    }

    /// Stop reception and join callbacks before releasing buffered samples.
    pub fn stop_rx(&mut self) -> Result<()> {
        let result = Error::check(ffi::airspy_stop_rx(self.handle()));
        // libairspy joins the callback threads even when it reports failure,
        // so nothing can still read the state released here.
        self.rx_ctx.take();
        result
    }

    /// Check if the Airspy is currently streaming.
    pub fn is_streaming(&mut self) -> bool {
        ffi::airspy_is_streaming(self.handle()) == ffi::AIRSPY_TRUE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr;
    use std::thread;

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
        assert_eq!(unsafe { rx_cb(&mut transfer) }, ffi::RX_CONTINUE);
        input[0].re = 99.0;
        assert_eq!(input[0].re, 99.0);
        let mut first = [Complex::default(); 1];
        assert_eq!(state.receive(&mut first, 0), Ok(1));
        assert_eq!(first[0], Complex::new(1.0_f32, -2.0));
        let mut remaining = [Complex::new(9.0, 9.0); 3];
        assert_eq!(state.receive(&mut remaining, 0), Ok(1));
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
    fn empty_receive_times_out_without_modifying_output() {
        let state = RxState::new();
        let mut output = [Complex::new(9.0, 9.0)];
        assert_eq!(state.receive(&mut output, 0), Err(Error::Timeout));
        assert_eq!(output[0], Complex::new(9.0_f32, 9.0));
        assert_eq!(state.receive(&mut [], 0), Err(Error::InvalidParameter));
    }

    #[test]
    fn overflow_is_terminal_even_with_queued_samples() {
        let state = RxState::new();
        state.push(&vec![Complex::default(); RX_CAPACITY]).unwrap();
        assert_eq!(state.push(&[Complex::default()]), Err(Error::Overflow));
        assert_eq!(
            state.receive(&mut [Complex::default()], 0),
            Err(Error::Overflow)
        );
        assert_eq!(state.push(&[]), Err(Error::Overflow));
        let fresh = RxState::new();
        fresh.push(&[Complex::new(1.0, 2.0)]).unwrap();
        assert_eq!(fresh.receive(&mut [Complex::default()], 0), Ok(1));
    }

    #[test]
    fn receive_accepts_samples_from_callback_thread() {
        let state = RxState::new();
        thread::scope(|scope| {
            scope.spawn(|| state.push(&[Complex::new(1.0, 2.0)]).unwrap());
            let mut output = [Complex::default()];
            assert_eq!(state.receive(&mut output, 5_000), Ok(1));
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
            state.receive(&mut [Complex::default()], 0),
            Err(Error::Thread)
        );
    }

    #[test]
    #[ignore = "requires an attached Airspy device; run with RUST_TEST_THREADS=1"]
    fn stop_releases_state_and_allows_restart() {
        let mut device = Airspy::open().unwrap();
        let mut samples = [Complex::default(); 1024];
        assert_eq!(
            device.receive(&mut samples, 0),
            Err(Error::StreamingStopped)
        );
        for _ in 0..2 {
            device.start_rx().unwrap();
            assert!(device.receive(&mut samples, 5_000).unwrap() > 0);
            device.stop_rx().unwrap();
            assert_eq!(
                device.receive(&mut samples, 0),
                Err(Error::StreamingStopped)
            );
        }
    }
}
