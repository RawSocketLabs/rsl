use crate::{ffi, Airspy, Error, Result};
use num_complex::Complex;
use std::ffi::{c_int, c_void};
use std::sync::mpsc::Sender;

/// Complex IQ data; reception produces `IQ<f32>`.
pub type IQ<T> = Complex<T>;

/// Callback in Rust to send to the libairspy C library that sends buffers
/// through to a user channel, quitting streaming when the channel hangs up.
unsafe extern "C" fn rx_cb(transfer: *mut ffi::airspy_transfer_t) -> c_int {
    // libairspy owns the transfer and sample storage for this callback.
    if transfer.is_null() {
        return 1;
    }
    let transfer = unsafe { &*transfer };
    if transfer.sample_count <= 0 {
        return 0;
    }
    if transfer.samples.is_null() || transfer.ctx.is_null() {
        return 1;
    }
    let samples = unsafe {
        std::slice::from_raw_parts(
            transfer.samples.cast::<IQ<f32>>(),
            transfer.sample_count as usize,
        )
    };
    // The boxed sender stays alive until stop/close joins this thread.
    let sender = unsafe { &*transfer.ctx.cast::<Sender<Vec<IQ<f32>>>>() };
    i32::from(sender.send(samples.to_vec()).is_err())
}

impl Airspy {
    /// Start floating-point IQ reception; receiver disconnection requests a stop.
    pub fn start_rx(&mut self, sender: Sender<Vec<IQ<f32>>>) -> Result<()> {
        if self.rx_ctx.is_some() {
            self.stop_rx()?;
        }
        Error::check(unsafe { ffi::airspy_set_sample_type(self.ptr, 0) })?;
        let mut sender = Box::new(sender);
        let ctx = (&mut *sender as *mut Sender<Vec<IQ<f32>>>).cast::<c_void>();
        self.rx_ctx = Some(sender);
        let result = Error::check(unsafe { ffi::airspy_start_rx(self.ptr, rx_cb, ctx) });
        if result.is_err() {
            // Join any threads created before a partial startup failure.
            let _ = self.stop_rx();
        }
        result
    }

    /// Stop reception and join callback threads before releasing their sender.
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
    use crate::init;
    #[test]
    fn callback_copies_iq_and_stops_after_disconnect() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut sender = Box::new(tx);
        let mut samples = [IQ::new(1.0_f32, -2.0), IQ::new(3.0, -4.0)];
        let mut transfer = ffi::airspy_transfer_t {
            device: std::ptr::null_mut(),
            ctx: (&mut *sender as *mut Sender<Vec<IQ<f32>>>).cast(),
            samples: samples.as_ptr().cast(),
            sample_count: 2,
        };
        assert_eq!(unsafe { rx_cb(&mut transfer) }, 0);
        samples[0].re = 99.0;
        transfer.samples = samples.as_ptr().cast();
        assert_eq!(
            rx.recv().unwrap(),
            vec![IQ::new(1.0_f32, -2.0), IQ::new(3.0, -4.0)]
        );
        drop(rx);
        assert_eq!(unsafe { rx_cb(&mut transfer) }, 1);
    }

    #[test]
    fn releasing_context_disconnects_receiver() {
        let (tx, rx) = std::sync::mpsc::channel::<Vec<IQ<f32>>>();
        let sender = Box::new(tx);
        drop(sender);
        assert!(rx.recv().is_err());
    }

    #[test]
    #[ignore = "requires an attached Airspy device; run with RUST_TEST_THREADS=1"]
    fn test_start_rx() {
        let _ = init();
        let mut airspy = Airspy::new().unwrap();
        let (tx, _) = std::sync::mpsc::channel();
        assert!(airspy.start_rx(tx).is_ok());
    }

    #[test]
    #[ignore = "requires an attached Airspy device; run with RUST_TEST_THREADS=1"]
    fn stop_releases_sender_and_allows_restart() {
        let mut device = Airspy::new().unwrap();
        for _ in 0..2 {
            let (tx, rx) = std::sync::mpsc::channel();
            device.start_rx(tx).unwrap();
            device.stop_rx().unwrap();
            // Drain samples already queued before stop, then require disconnection.
            loop {
                match rx.try_recv() {
                    Ok(_) => continue,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
                    Err(std::sync::mpsc::TryRecvError::Empty) => panic!("stop retained sender"),
                }
            }
        }
    }

    #[test]
    #[ignore = "requires an attached Airspy device; run with RUST_TEST_THREADS=1"]
    fn test_is_streaming() {
        let _ = init();
        let mut airspy = Airspy::new().unwrap();
        assert!(!airspy.is_streaming());
    }
}
