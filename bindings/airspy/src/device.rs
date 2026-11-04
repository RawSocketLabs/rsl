use crate::rx::RxState;
use crate::{ffi, Error, Result};
use std::ffi::CStr;
use std::sync::Arc;

/// Store a reference to an opened Airspy device.
pub struct Airspy {
    pub(crate) ptr: *mut ffi::AirspyDevice,
    /// Stable reception state, retained until the C threads have joined.
    pub(crate) rx_ctx: Option<Arc<RxState>>,
}

/// Ensure whenever the Airspy goes out of scope it is closed.
impl Drop for Airspy {
    fn drop(&mut self) {
        // airspy_close stops any active RX (cancelling transfers and joining
        // the streaming threads) before freeing the device, so afterwards the
        // callback context is unreachable; Rust drops it after this returns.
        unsafe {
            ffi::airspy_close(self.ptr);
        }
    }
}

/// Hold an Airspy's part ID and serial number.
pub struct PartID {
    pub part_id: [u32; 2],
    pub serial_no: u64,
}

impl Airspy {
    /// Try to open the next available Airspy device.
    pub fn open() -> Result<Self> {
        let mut device: Airspy = Airspy {
            ptr: std::ptr::null_mut(),
            rx_ctx: None,
        };
        Error::check(unsafe { ffi::airspy_open(&mut device.ptr) })?;
        Ok(device)
    }

    /// Try to open a specific Airspy device by serial number.
    pub fn open_serial(serial_number: u64) -> Result<Self> {
        let mut device: Airspy = Airspy {
            ptr: std::ptr::null_mut(),
            rx_ctx: None,
        };
        Error::check(unsafe { ffi::airspy_open_sn(&mut device.ptr, serial_number) })?;
        Ok(device)
    }

    /// Get available sample rates for this Airspy.
    pub fn get_sample_rates(&mut self) -> Result<Vec<u32>> {
        let mut len: u32 = 0;
        Error::check(unsafe { ffi::airspy_get_samplerates(self.ptr, &mut len, 0) })?;
        let mut rates = vec![0; len as usize];
        Error::check(unsafe { ffi::airspy_get_samplerates(self.ptr, rates.as_mut_ptr(), len) })?;
        Ok(rates)
    }

    /// Set this Airspy's sample rate to a specific rate.
    ///
    /// This rate must be in the available rates or an error is returned.
    pub fn set_sample_rate(&mut self, target_rate: u32) -> Result<()> {
        let rates = self.get_sample_rates()?;
        for (idx, rate) in rates.iter().enumerate() {
            if *rate == target_rate {
                return Error::check(unsafe { ffi::airspy_set_samplerate(self.ptr, idx as u32) });
            }
        }
        Err(Error::InvalidParameter)
    }

    /// Get the Airspy board type.
    pub fn get_board_id(&mut self) -> Result<&'static str> {
        let mut id: u8 = 0;
        Error::check(unsafe { ffi::airspy_board_id_read(self.ptr, &mut id as *mut u8) })?;
        // libairspy returns a pointer into its static name table.
        let cstr: &'static CStr = unsafe { CStr::from_ptr(ffi::airspy_board_id_name(id.into())) };
        Ok(cstr.to_str().unwrap_or("unknown"))
    }

    /// Get the Airspy firmware version number.
    pub fn get_version(&mut self) -> Result<String> {
        let mut buf = [0; 255];
        let bufp = buf.as_mut_ptr();
        Error::check(unsafe { ffi::airspy_version_string_read(self.ptr, bufp, 255) })?;
        let cstr = unsafe { CStr::from_ptr(buf.as_ptr()) };
        Ok(cstr.to_string_lossy().into_owned())
    }

    /// Get the Airspy part ID and serial number.
    pub fn get_partid_serial(&mut self) -> Result<PartID> {
        let mut v = ffi::airspy_read_partid_serialno_t {
            part_id: [0u32; 2],
            serial_no: [0u32; 4],
        };
        Error::check(unsafe { ffi::airspy_board_partid_serialno_read(self.ptr, &mut v) })?;
        Ok(PartID {
            part_id: v.part_id,
            serial_no: (v.serial_no[2] as u64) << 32 | v.serial_no[3] as u64,
        })
    }

    /// Set centre frequency in Hz (24 MHz to 1.75 GHz).
    pub fn set_freq(&mut self, freq: u32) -> Result<()> {
        Error::check(unsafe { ffi::airspy_set_freq(self.ptr, freq) })
    }

    /// Set the LNA gain step index; libairspy clamps out-of-range values.
    pub fn set_lna_gain(&mut self, gain: u8) -> Result<()> {
        Error::check(unsafe { ffi::airspy_set_lna_gain(self.ptr, gain) })
    }

    /// Set the mixer gain step index; libairspy clamps out-of-range values.
    pub fn set_mixer_gain(&mut self, gain: u8) -> Result<()> {
        Error::check(unsafe { ffi::airspy_set_mixer_gain(self.ptr, gain) })
    }

    /// Set the VGA gain step index; libairspy clamps out-of-range values.
    pub fn set_vga_gain(&mut self, gain: u8) -> Result<()> {
        Error::check(unsafe { ffi::airspy_set_vga_gain(self.ptr, gain) })
    }

    /// Enable/disable LNA AGC
    pub fn set_lna_agc(&mut self, enable: bool) -> Result<()> {
        Error::check(unsafe { ffi::airspy_set_lna_agc(self.ptr, enable as u8) })
    }

    /// Enable/disable mixer AGC
    pub fn set_mixer_agc(&mut self, enable: bool) -> Result<()> {
        Error::check(unsafe { ffi::airspy_set_mixer_agc(self.ptr, enable as u8) })
    }

    /// Enable/disable RF bias voltage
    pub fn set_rf_bias(&mut self, enable: bool) -> Result<()> {
        Error::check(unsafe { ffi::airspy_set_rf_bias(self.ptr, enable as u8) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires an attached Airspy device; run with RUST_TEST_THREADS=1"]
    fn discovers_device_and_available_rates() {
        let mut device = Airspy::open().unwrap();
        assert!(!device.get_board_id().unwrap().is_empty());
        assert!(!device.get_version().unwrap().is_empty());
        let rates = device.get_sample_rates().unwrap();
        assert!(!rates.is_empty());
        assert!(rates.iter().all(|rate| *rate > 0));
    }

    #[test]
    #[ignore = "requires an attached Airspy device; run with RUST_TEST_THREADS=1"]
    fn configures_device_with_advertised_rate() {
        let mut device = Airspy::open().unwrap();
        let rates = device.get_sample_rates().unwrap();
        let rate = *rates.first().expect("device must advertise a sample rate");
        device.set_sample_rate(rate).unwrap();
        device.set_freq(434_000_000).unwrap();
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
            device.get_partid_serial().unwrap()
        };
        let mut reopened = Airspy::open_serial(identity.serial_no).unwrap();
        let actual = reopened.get_partid_serial().unwrap();
        assert_eq!(actual.serial_no, identity.serial_no);
        assert_eq!(actual.part_id, identity.part_id);
    }
}
