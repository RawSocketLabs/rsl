use cxx::UniquePtr;

#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("usdr_wrapper.hpp");

        type UsdrDevice;

        fn make_usdr_device(
            device_string: &CxxString,
            loglevel: i32,
            samplerate_rx: u32,
            samples_per_packet: u32,
        ) -> UniquePtr<UsdrDevice>;

        fn start(self: Pin<&mut UsdrDevice>);
        fn stop(self: Pin<&mut UsdrDevice>);
        fn set_rx_freq(self: Pin<&mut UsdrDevice>, hz: u32);
        fn set_rx_bandwidth(self: Pin<&mut UsdrDevice>, hz: u32);

        unsafe fn receive_data(
            self: Pin<&mut UsdrDevice>,
            ch1: *mut u8,
            ch2: *mut u8,
            samples: u32,
        );

        fn rx_bytes_per_sample(self: &UsdrDevice) -> u32;
    }
}

pub use ffi::UsdrDevice;
use std::pin::Pin;
use num_complex::Complex;

/// IQ sample in ci16 format (complex int16)
#[derive(Debug, Clone, Copy, Default)]
#[repr(C)]
pub struct IQSample {
    pub i: i16,
    pub q: i16,
}

impl IQSample {
    /// Size of an IQSample in bytes
    pub const SIZE: usize = 4; // 2 bytes for i + 2 bytes for q

    /// Convert an IQSample to its byte representation
    pub fn to_bytes(&self) -> [u8; 4] {
        let i_bytes = self.i.to_le_bytes();
        let q_bytes = self.q.to_le_bytes();
        [i_bytes[0], i_bytes[1], q_bytes[0], q_bytes[1]]
    }
}

/// Convert a slice of IQSamples to a Vec of bytes (safe, no unsafe code)
pub fn samples_to_bytes(samples: &[Complex<i16>]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        bytes.extend_from_slice(sample.re.to_le_bytes().as_slice());
        bytes.extend_from_slice(sample.im.to_le_bytes().as_slice());
    }
    bytes
}

/// Error type for USDR operations
#[derive(Debug)]
pub enum UsdrError {
    /// Buffer too small for the requested number of samples
    BufferTooSmall { required: usize, provided: usize },
    /// Device is null
    NullDevice,
}

impl std::fmt::Display for UsdrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UsdrError::BufferTooSmall { required, provided } => {
                write!(f, "Buffer too small: need {} bytes, got {}", required, provided)
            }
            UsdrError::NullDevice => write!(f, "Device is null"),
        }
    }
}

impl std::error::Error for UsdrError {}

/// Safe wrapper around UsdrDevice that provides safe methods for receiving data
pub struct Device {
    inner: UniquePtr<UsdrDevice>,
    bytes_per_sample: u32,
}

impl Device {
    /// Create a new SafeUsdrDevice by opening a device
    pub fn open(
        device: &str,
        loglevel: i32,
        sr_rx: u32,
        spp: u32,
    ) -> Result<Self, UsdrError> {
        let inner = open_device(device, loglevel, sr_rx, spp);
        if inner.is_null() {
            return Err(UsdrError::NullDevice);
        }
        let bytes_per_sample = inner.as_ref().unwrap().rx_bytes_per_sample();
        Ok(Self { inner, bytes_per_sample })
    }

    /// Get bytes per sample for RX
    pub fn rx_bytes_per_sample(&self) -> u32 {
        self.bytes_per_sample
    }

    /// Start streaming
    pub fn start(&mut self) {
        self.inner.as_mut().unwrap().start();
    }

    /// Stop streaming
    pub fn stop(&mut self) {
        self.inner.as_mut().unwrap().stop();
    }

    /// Set RX frequency in Hz
    pub fn set_rx_freq(&mut self, hz: u32) {
        self.inner.as_mut().unwrap().set_rx_freq(hz);
    }

    /// Receive IQ samples into a slice (safe API)
    ///
    /// This method receives `samples.len()` IQ samples into the provided buffer.
    /// Each sample is 4 bytes (2 bytes I + 2 bytes Q for ci16 format).
    pub fn receive(&mut self, samples: &mut [Complex<i16>]) -> Result<usize, UsdrError> {
        let num_samples = samples.len() as u32;

        // Safety: IQSample is repr(C) and matches the ci16 layout (i16, i16)
        // We're passing a valid mutable pointer to properly sized memory
        let ptr = samples.as_mut_ptr() as *mut u8;

        unsafe {
            self.inner.as_mut().unwrap().receive_data(
                ptr,
                std::ptr::null_mut(),
                num_samples,
            );
        }

        Ok(samples.len())
    }

    /// Get mutable access to the underlying device (for advanced operations)
    pub fn inner_mut(&mut self) -> Pin<&mut UsdrDevice> {
        self.inner.as_mut().unwrap()
    }

    /// Get immutable access to the underlying device
    pub fn inner(&self) -> &UsdrDevice {
        self.inner.as_ref().unwrap()
    }
}

pub fn open_device(
    device: &str,
    loglevel: i32,
    sr_rx: u32,
    spp: u32,
) -> UniquePtr<UsdrDevice> {
    cxx::let_cxx_string!(device_cxx = device);
    ffi::make_usdr_device(
        &device_cxx,
        loglevel,
        sr_rx,
        spp,
    )
}


#[cfg(test)]
mod tests {
    use crate::{Device, IQSample, samples_to_bytes};
    use std::fs::File;
    use std::io::Write;
    use std::time::{Duration, Instant};
    use num_complex::Complex;

    #[test]
    fn test_receive_samples() {
        // Open device with safe wrapper
        let mut device = Device::open(
            "",           // device string (empty = auto-detect)
            3,            // loglevel
            1_000_000,    // samplerate_rx (1 MHz)
            1024,         // samples_per_packet
        ).expect("Failed to open USDR device");

        println!("RX bytes per sample: {}", device.rx_bytes_per_sample());

        // Start streaming
        device.start();
        device.set_rx_freq(104_100_000);

        // Allocate buffer for receiving samples using safe IQSample type
        let num_samples: usize = 1024;
        let mut samples: Vec<Complex<i16>> = vec![Complex::default(); num_samples];

        // Open output file
        let mut output_file = File::create("/tmp/out").expect("Failed to create output file /tmp/out");

        // Capture for 10 seconds
        let capture_duration = Duration::from_secs(10);
        let start_time = Instant::now();
        let mut total_samples: u64 = 0;
        let mut total_bytes: u64 = 0;

        println!("Starting 10 second capture to /tmp/out...");

        while start_time.elapsed() < capture_duration {
            // Receive samples using safe API
            device.receive(&mut samples).expect("Failed to receive samples");

            // Write raw IQ data to file using safe conversion
            let bytes = samples_to_bytes(&samples);
            output_file.write_all(&bytes).expect("Failed to write to output file");

            total_samples += num_samples as u64;
            total_bytes += bytes.len() as u64;
        }

        // Flush and close file
        output_file.flush().expect("Failed to flush output file");

        // Stop streaming
        device.stop();

        let elapsed = start_time.elapsed();
        println!("Capture complete!");
        println!("  Duration: {:.2} seconds", elapsed.as_secs_f64());
        println!("  Total samples: {}", total_samples);
        println!("  Total bytes: {} ({:.2} MB)", total_bytes, total_bytes as f64 / 1_000_000.0);
        println!("  Effective sample rate: {:.2} Hz", total_samples as f64 / elapsed.as_secs_f64());
        println!("  Output file: /tmp/out");

        // Basic sanity check
        assert!(total_samples > 0, "Expected to receive some samples");
        assert!(total_bytes > 0, "Expected to write some data");
    }
}