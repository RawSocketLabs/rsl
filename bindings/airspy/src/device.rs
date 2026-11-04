use crate::rx::RxState;
use crate::{Error, Result, ffi};
use std::ffi::{CStr, c_int};
use std::ptr::{self, NonNull};
use std::sync::Arc;

/// Firmware version buffer size; libairspy's own limit is smaller.
const VERSION_LEN: u8 = u8::MAX;

/// Store a reference to an opened Airspy device.
pub struct Airspy {
    /// Live libairspy handle: set only by a successful open, closed only in
    /// `Drop`.
    ptr: NonNull<ffi::AirspyDevice>,
    /// Stable reception state, retained until the C threads have joined.
    pub(crate) rx_ctx: Option<Arc<RxState>>,
}

/// Ensure whenever the Airspy goes out of scope it is closed.
impl Drop for Airspy {
    fn drop(&mut self) {
        // SAFETY: `ptr` is live (field invariant) and unused after this.
        // airspy_close stops RX and joins the streaming threads before freeing
        // the device, so the callback context Rust drops next is unreachable.
        unsafe {
            ffi::airspy_close(self.ptr.as_ptr());
        }
    }
}

/// An Airspy board's part ID and serial number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Identity {
    pub part_id: [u32; 2],
    pub serial: u64,
}

impl Airspy {
    /// Try to open the next available Airspy device.
    pub fn open() -> Result<Self> {
        Self::open_with(|device| ffi::airspy_open(device))
    }

    /// Try to open a specific Airspy device by serial number.
    pub fn open_serial(serial_number: u64) -> Result<Self> {
        Self::open_with(|device| ffi::airspy_open_sn(device, serial_number))
    }

    /// Run a libairspy open call and take ownership of the handle it returns.
    fn open_with(open: impl FnOnce(&mut *mut ffi::AirspyDevice) -> c_int) -> Result<Self> {
        let mut device = ptr::null_mut();
        Error::check(open(&mut device))?;
        // libairspy reports success only alongside a handle.
        let ptr = NonNull::new(device).ok_or(Error::Other)?;
        Ok(Self { ptr, rx_ctx: None })
    }

    /// Borrow the live handle for a libairspy call.
    pub(crate) fn handle(&mut self) -> &mut ffi::AirspyDevice {
        // SAFETY: `ptr` is live (field invariant). The type is zero-sized and
        // aligned to 1, so the borrow covers none of libairspy's bytes.
        unsafe { self.ptr.as_mut() }
    }

    /// Get available sample rates for this Airspy.
    pub fn sample_rates(&mut self) -> Result<Vec<u32>> {
        let mut count: u32 = 0;
        // SAFETY: with `len == 0`, libairspy writes only the count, to `count`.
        Error::check(unsafe { ffi::airspy_get_samplerates(self.handle(), &mut count, 0) })?;
        let mut rates = vec![0; count as usize];
        // SAFETY: `rates` holds exactly `count` writable elements.
        Error::check(unsafe {
            ffi::airspy_get_samplerates(self.handle(), rates.as_mut_ptr(), count)
        })?;
        Ok(rates)
    }

    /// Set this Airspy's sample rate to a specific rate.
    ///
    /// This rate must be in the available rates or an error is returned.
    pub fn set_sample_rate(&mut self, target_rate: u32) -> Result<()> {
        let index = self
            .sample_rates()?
            .iter()
            .zip(0u32..)
            .find_map(|(&rate, index)| (rate == target_rate).then_some(index))
            .ok_or(Error::InvalidParameter)?;
        Error::check(ffi::airspy_set_samplerate(self.handle(), index))
    }

    /// Read the board's name from libairspy's board table.
    pub fn board_name(&mut self) -> Result<&'static str> {
        let mut id: u8 = 0;
        Error::check(ffi::airspy_board_id_read(self.handle(), &mut id))?;
        let name = ffi::airspy_board_id_name(id.into());
        // SAFETY: libairspy returns a NUL-terminated entry of its static name
        // table for every id.
        let name: &'static CStr = unsafe { CStr::from_ptr(name) };
        Ok(name.to_str().unwrap_or("unknown"))
    }

    /// Read the firmware version string.
    pub fn firmware_version(&mut self) -> Result<String> {
        let mut buf = [0u8; VERSION_LEN as usize];
        // SAFETY: `buf` holds exactly `VERSION_LEN` writable bytes.
        Error::check(unsafe {
            ffi::airspy_version_string_read(self.handle(), buf.as_mut_ptr().cast(), VERSION_LEN)
        })?;
        let version = CStr::from_bytes_until_nul(&buf).map_err(|_| Error::Other)?;
        Ok(version.to_string_lossy().into_owned())
    }

    /// Read the board's part ID and serial number.
    pub fn identity(&mut self) -> Result<Identity> {
        let mut v = ffi::airspy_read_partid_serialno_t {
            part_id: [0u32; 2],
            serial_no: [0u32; 4],
        };
        Error::check(ffi::airspy_board_partid_serialno_read(
            self.handle(),
            &mut v,
        ))?;
        Ok(Identity {
            part_id: v.part_id,
            serial: (u64::from(v.serial_no[2]) << 32) | u64::from(v.serial_no[3]),
        })
    }

    /// Set centre frequency in Hz (24 MHz to 1.75 GHz).
    pub fn set_frequency(&mut self, freq: u32) -> Result<()> {
        Error::check(ffi::airspy_set_freq(self.handle(), freq))
    }

    /// Set the LNA gain step index; libairspy clamps out-of-range values.
    pub fn set_lna_gain(&mut self, gain: u8) -> Result<()> {
        Error::check(ffi::airspy_set_lna_gain(self.handle(), gain))
    }

    /// Set the mixer gain step index; libairspy clamps out-of-range values.
    pub fn set_mixer_gain(&mut self, gain: u8) -> Result<()> {
        Error::check(ffi::airspy_set_mixer_gain(self.handle(), gain))
    }

    /// Set the VGA gain step index; libairspy clamps out-of-range values.
    pub fn set_vga_gain(&mut self, gain: u8) -> Result<()> {
        Error::check(ffi::airspy_set_vga_gain(self.handle(), gain))
    }

    /// Enable/disable LNA AGC
    pub fn set_lna_agc(&mut self, enable: bool) -> Result<()> {
        Error::check(ffi::airspy_set_lna_agc(self.handle(), u8::from(enable)))
    }

    /// Enable/disable mixer AGC
    pub fn set_mixer_agc(&mut self, enable: bool) -> Result<()> {
        Error::check(ffi::airspy_set_mixer_agc(self.handle(), u8::from(enable)))
    }

    /// Enable/disable RF bias voltage
    pub fn set_rf_bias(&mut self, enable: bool) -> Result<()> {
        Error::check(ffi::airspy_set_rf_bias(self.handle(), u8::from(enable)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires an attached Airspy device; run with RUST_TEST_THREADS=1"]
    fn discovers_device_and_available_rates() {
        let mut device = Airspy::open().unwrap();
        assert!(!device.board_name().unwrap().is_empty());
        assert!(!device.firmware_version().unwrap().is_empty());
        let rates = device.sample_rates().unwrap();
        assert!(!rates.is_empty());
        assert!(rates.iter().all(|rate| *rate > 0));
    }

    #[test]
    #[ignore = "requires an attached Airspy device; run with RUST_TEST_THREADS=1"]
    fn configures_device_with_advertised_rate() {
        let mut device = Airspy::open().unwrap();
        let rates = device.sample_rates().unwrap();
        let rate = *rates.first().expect("device must advertise a sample rate");
        device.set_sample_rate(rate).unwrap();
        device.set_frequency(434_000_000).unwrap();
        device.set_lna_gain(7).unwrap();
        device.set_mixer_gain(7).unwrap();
        device.set_vga_gain(7).unwrap();
        device.set_lna_agc(true).unwrap();
        device.set_mixer_agc(true).unwrap();
        device.set_rf_bias(false).unwrap();
    }

    #[test]
    #[ignore = "requires an attached Airspy device; run with RUST_TEST_THREADS=1"]
    fn reopens_same_device_by_serial() {
        let identity = {
            let mut device = Airspy::open().unwrap();
            device.identity().unwrap()
        };
        let mut reopened = Airspy::open_serial(identity.serial).unwrap();
        let actual = reopened.identity().unwrap();
        assert_eq!(actual, identity);
    }
}
