use cxx::UniquePtr;

#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("usdr_wrapper.hpp");

        type UsdrDevice;

        fn make_usdr_device(
            device_string: &CxxString,
            loglevel: i32,
            samples_per_packet: u32,
        ) -> Result<UniquePtr<UsdrDevice>>;

        fn start(self: Pin<&mut UsdrDevice>, rate: u32) -> u32;
        fn stop(self: Pin<&mut UsdrDevice>);
        fn set_rx_freq(self: Pin<&mut UsdrDevice>, hz: u32);
        fn set_rx_bandwidth(self: Pin<&mut UsdrDevice>, hz: u32);
        fn get_temperature(self: Pin<&mut UsdrDevice>) -> Result<f32>;

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
use num_complex::Complex;
use std::pin::Pin;

/// Convert a slice of IQ samples to a Vec of bytes
pub fn samples_to_bytes(samples: &[Complex<i16>]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 4);
    for sample in samples {
        bytes.extend_from_slice(&sample.re.to_le_bytes());
        bytes.extend_from_slice(&sample.im.to_le_bytes());
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
    /// Device is too hot!
    TooHot,
    /// Failed to create device
    CreateDevice,
    /// Failed to power on device
    PowerOn,
    /// Failed to set sample rate
    SetSampleRate,
    /// Failed to create RX stream
    CreateRxStream,
    /// Failed to get RX stream info
    GetRxStreamInfo,
    /// Failed to sync RX stream
    SyncOff,
    /// Failed to pre-charge RX stream
    RxStreamPreCharge,
    /// Failed to set frequency
    SetFreq,
    /// Failed to set bandwidth
    SetBandwidth,
    /// Failed to sync RX stream to off
    SyncNone,
    /// Failed to get temperature
    GetTemperature,
}

const USDR_SUCCESS: u32                 = 0;
const USDR_ERR_CREATE_DEVICE: u32       = 1;
const USDR_ERR_POWER_ON: u32            = 2;
const USDR_ERR_SET_SAMPLE_RATE: u32     = 3;
const USDR_ERR_CREATE_RX_STREAM: u32    = 4;
const USDR_ERR_GET_RX_STREAM_INFO: u32  = 5;
const USDR_ERR_SYNC_OFF: u32            = 6;
const USDR_ERR_RX_STREAM_PRE_CHARGE: u32 = 7;
const USDR_ERR_NULL_DEVICE: u32         = 8;
const USDR_ERR_SET_FREQ: u32            = 9;
const USDR_ERR_SET_BANDWIDTH: u32       = 10;
const USDR_ERR_SYNC_NONE: u32           = 11;
const USDR_ERR_TOO_HOT: u32             = 12;

impl std::fmt::Display for UsdrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UsdrError::BufferTooSmall { required, provided } => {
                write!(f, "Buffer too small: need {} bytes, got {}", required, provided)
            }
            UsdrError::NullDevice => write!(f, "Device is null"),
            _ => write!(f, "{:?}", self),
        }
    }
}

impl std::error::Error for UsdrError {}

/// Safe wrapper around UsdrDevice
pub struct Device {
    inner: UniquePtr<UsdrDevice>,
    bytes_per_sample: u32,
}

impl Device {
    /// Create a new Device by opening a USDR device
    pub fn open(
        device: &str,
        loglevel: i32,
        spp: u32,
    ) -> Result<Self, UsdrError> {
        let inner = open_device(device, loglevel, spp).map_err(|_| UsdrError::NullDevice)?;
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
    pub fn start(&mut self, rate: u32) -> Result<(), UsdrError> {
        match self.inner.as_mut().expect("Device is null").start(rate) {
            USDR_SUCCESS => Ok(()),
            USDR_ERR_CREATE_DEVICE => Err(UsdrError::CreateDevice),
            USDR_ERR_POWER_ON => Err(UsdrError::PowerOn),
            USDR_ERR_SET_SAMPLE_RATE => Err(UsdrError::SetSampleRate),
            USDR_ERR_CREATE_RX_STREAM => Err(UsdrError::CreateRxStream),
            USDR_ERR_GET_RX_STREAM_INFO => Err(UsdrError::GetRxStreamInfo),
            USDR_ERR_SYNC_OFF => Err(UsdrError::SyncOff),
            USDR_ERR_RX_STREAM_PRE_CHARGE => Err(UsdrError::RxStreamPreCharge),
            USDR_ERR_NULL_DEVICE => Err(UsdrError::NullDevice),
            USDR_ERR_SET_FREQ => Err(UsdrError::SetFreq),
            USDR_ERR_SET_BANDWIDTH => Err(UsdrError::SetBandwidth),
            USDR_ERR_SYNC_NONE => Err(UsdrError::SyncNone),
            USDR_ERR_TOO_HOT => Err(UsdrError::TooHot),
            _ => panic!("Unexpected return value from USDR start()"),
        }
    }

    /// Stop streaming
    pub fn stop(&mut self) {
        self.inner.as_mut().expect("Device is null").stop();
    }

    /// Set RX frequency in Hz
    pub fn set_rx_freq(&mut self, hz: u32) {
        self.inner.as_mut().expect("Device is null").set_rx_freq(hz);
    }

    /// Get device temperature in degrees Celsius
    pub fn get_temperature(&mut self) -> Result<f32, UsdrError> {
        self.inner
            .as_mut()
            .expect("Device is null")
            .get_temperature()
            .map_err(|_| UsdrError::GetTemperature)
    }

    /// Receive IQ samples into a slice
    ///
    /// Each sample is 4 bytes (2 bytes I + 2 bytes Q for ci16 format).
    pub fn receive(&mut self, samples: &mut [Complex<i16>]) -> Result<usize, UsdrError> {
        let num_samples = samples.len() as u32;
        let ptr = samples.as_mut_ptr() as *mut u8;

        unsafe {
            self.inner
                .as_mut()
                .expect("Device is null")
                .receive_data(ptr, std::ptr::null_mut(), num_samples);
        }

        Ok(samples.len())
    }

    /// Get mutable access to the underlying device
    pub fn inner_mut(&mut self) -> Pin<&mut UsdrDevice> {
        self.inner.as_mut().expect("Device is null")
    }

    /// Get immutable access to the underlying device
    pub fn inner(&self) -> &UsdrDevice {
        self.inner.as_ref().expect("Device is null")
    }
}

/// Open a USDR device and return the raw UniquePtr
pub fn open_device(
    device: &str,
    loglevel: i32,
    spp: u32,
) -> Result<UniquePtr<UsdrDevice>, cxx::Exception> {
    cxx::let_cxx_string!(device_cxx = device);
    ffi::make_usdr_device(&device_cxx, loglevel, spp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use std::time::{Duration, Instant};

    #[test]
    fn test_receive_samples() {
        let mut device = Device::open(
            "",           // device string (empty = auto-detect)
            3,            // loglevel
            1024,         // samples_per_packet
        ).expect("Failed to open USDR device");

        println!("RX bytes per sample: {}", device.rx_bytes_per_sample());

        device.start(1_000_000);
        device.set_rx_freq(104_100_000);

        let num_samples: usize = 1024;
        let mut samples: Vec<Complex<i16>> = vec![Complex::default(); num_samples];

        let mut output_file = File::create("/tmp/out").expect("Failed to create output file");

        let capture_duration = Duration::from_secs(10);
        let start_time = Instant::now();
        let mut total_samples: u64 = 0;
        let mut total_bytes: u64 = 0;

        println!("Starting 10 second capture to /tmp/out...");

        while start_time.elapsed() < capture_duration {
            device.receive(&mut samples).expect("Failed to receive samples");

            let bytes = samples_to_bytes(&samples);
            output_file.write_all(&bytes).expect("Failed to write to output file");

            total_samples += num_samples as u64;
            total_bytes += bytes.len() as u64;
        }

        output_file.flush().expect("Failed to flush output file");
        device.stop();

        let elapsed = start_time.elapsed();
        println!("Capture complete!");
        println!("  Duration: {:.2} seconds", elapsed.as_secs_f64());
        println!("  Total samples: {}", total_samples);
        println!("  Total bytes: {} ({:.2} MB)", total_bytes, total_bytes as f64 / 1_000_000.0);
        println!("  Effective sample rate: {:.2} Hz", total_samples as f64 / elapsed.as_secs_f64());

        assert!(total_samples > 0, "Expected to receive some samples");
        assert!(total_bytes > 0, "Expected to write some data");
    }
}