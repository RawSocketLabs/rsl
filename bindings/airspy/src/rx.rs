use crate::{ffi, Airspy, Error, Result};
use num_complex::Complex;
use std::collections::VecDeque;
use std::ffi::{c_int, c_void};
use std::slice;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

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

    /// Copy available samples, waiting up to the deadline when empty.
    fn receive(&self, samples: &mut [Complex<f32>], timeout_ms: u32) -> Result<usize> {
        if samples.is_empty() {
            return Err(Error::InvalidParameter);
        }
        let deadline = Instant::now() + Duration::from_millis(timeout_ms.into());
        let buffer = self.buffer.lock().map_err(|_| Error::Thread)?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        let (mut buffer, _) = self
            .ready
            .wait_timeout_while(buffer, remaining, |buffer| {
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
unsafe extern "C" fn rx_cb(transfer: *mut ffi::airspy_transfer_t) -> c_int {
    if transfer.is_null() {
        return 1;
    }
    // libairspy owns the transfer and sample storage for this callback.
    let transfer = unsafe { &*transfer };
    if transfer.sample_count <= 0 {
        return 0;
    }
    if transfer.samples.is_null() || transfer.ctx.is_null() {
        return 1;
    }
    // start_rx selected float IQ; the C buffer contains sample_count pairs.
    let samples = unsafe {
        slice::from_raw_parts(
            transfer.samples.cast::<Complex<f32>>(),
            transfer.sample_count as usize,
        )
    };
    // The box stays alive until stop/close joins this callback's thread.
    let state = unsafe { &*transfer.ctx.cast::<RxState>() };
    i32::from(state.push(samples).is_err())
}

impl Airspy {
    /// Start float IQ reception with a fresh 262,144-sample private buffer.
    ///
    /// Calling this again synchronously stops the previous session first.
    pub fn start_rx(&mut self) -> Result<()> {
        if self.rx_ctx.is_some() {
            self.stop_rx()?;
        }
        Error::check(unsafe { ffi::airspy_set_sample_type(self.ptr, 0) })?;
        let mut state = Box::new(RxState::new());
        let ctx = (&mut *state as *mut RxState).cast::<c_void>();
        self.rx_ctx = Some(state);
        let result = Error::check(unsafe { ffi::airspy_start_rx(self.ptr, rx_cb, ctx) });
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
        let result = Error::check(unsafe { ffi::airspy_stop_rx(self.ptr) });
        self.rx_ctx.take();
        result
    }

    /// Check if the Airspy is currently streaming.
    pub fn is_streaming(&mut self) -> bool {
        unsafe { ffi::airspy_is_streaming(self.ptr) == 1 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn callback_copies_samples_and_preserves_remainder() {
        let mut state = Box::new(RxState::new());
        let mut input = [Complex::new(1.0_f32, -2.0), Complex::new(3.0, -4.0)];
        let mut transfer = ffi::airspy_transfer_t {
            device: std::ptr::null_mut(),
            ctx: (&mut *state as *mut RxState).cast(),
            samples: input.as_ptr().cast(),
            sample_count: 2,
        };
        assert_eq!(unsafe { rx_cb(&mut transfer) }, 0);
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
