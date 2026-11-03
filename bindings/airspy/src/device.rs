use crate::{ffi, Error, Result, IQ};
use std::ffi::CStr;
use std::sync::mpsc::Sender;

/// Store a reference to an opened Airspy device.
pub struct Airspy {
    pub(crate) ptr: *mut ffi::AirspyDevice,
    /// Stable callback context, retained until the C threads have joined.
    pub(crate) rx_ctx: Option<Box<Sender<Vec<IQ<f32>>>>>,
}

/// Ensure whenever the Airspy goes out of scope it is closed.
impl Drop for Airspy {
    #[inline(never)]
    fn drop(&mut self) {
        // airspy_close stops any active RX (cancelling transfers and joining
        // the streaming threads) before freeing the device, so afterwards the
        // callback context is unreachable and safe to reclaim.
        unsafe {
            ffi::airspy_close(self.ptr);
        }
        self.rx_ctx.take();
    }
}

/// Hold an Airspy's part ID and serial number.
pub struct PartID {
    pub part_id: [u32; 2],
    pub serial_no: u64,
}

impl Airspy {
    /// Try to open the next available Airspy device.
    pub fn new() -> Result<Airspy> {
        let mut device: Airspy = Airspy {
            ptr: std::ptr::null_mut(),
            rx_ctx: None,
        };
        Error::check(unsafe { ffi::airspy_open(&mut device.ptr) })?;
        Ok(device)
    }

    /// Try to open a specific Airspy device by serial number.
    pub fn from_serial(serial_number: u64) -> Result<Airspy> {
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
        let lenp = &mut len as *mut u32;
        Error::check(unsafe { ffi::airspy_get_samplerates(self.ptr, lenp, 0) })?;
        let mut rates: Vec<u32> = Vec::with_capacity(len as usize);
        let ratesp = rates.as_mut_ptr();
        Error::check(unsafe { ffi::airspy_get_samplerates(self.ptr, ratesp, len) })?;
        unsafe {
            rates.set_len(len as usize);
        }
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
    pub fn get_board_id(&mut self) -> Result<&str> {
        let mut id: u8 = 0;
        Error::check(unsafe { ffi::airspy_board_id_read(self.ptr, &mut id as *mut u8) })?;
        let cstr = unsafe { CStr::from_ptr(ffi::airspy_board_id_name(id.into())) };
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

    /// Set Airspy centre frequency, `freq` 24000000 to 1750000000 (in Hz)
    pub fn set_freq(&mut self, freq: u32) -> Result<()> {
        Error::check(unsafe { ffi::airspy_set_freq(self.ptr, freq) })
    }

    /// Set LNA gain, 0 to 15dB
    pub fn set_lna_gain(&mut self, gain: u8) -> Result<()> {
        Error::check(unsafe { ffi::airspy_set_lna_gain(self.ptr, gain) })
    }

    /// Set mixer gain, 0 to 15dB
    pub fn set_mixer_gain(&mut self, gain: u8) -> Result<()> {
        Error::check(unsafe { ffi::airspy_set_mixer_gain(self.ptr, gain) })
    }

    /// Set VGA gain, 0 to 15dB
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
    use crate::{exit, init};
    #[test]
    #[ignore = "requires an attached Airspy device; run with RUST_TEST_THREADS=1"]
    fn test_new() {
        let _ = init();
        let airspy = Airspy::new();
        assert!(airspy.is_ok());
        let _ = exit();
    }

    #[test]
    #[ignore = "requires an attached Airspy device; run with RUST_TEST_THREADS=1"]
    fn test_from_serial() {
        let _ = init();
        let serial: u64;
        {
            let mut airspy = Airspy::new().unwrap();
            let v = airspy.get_partid_serial().unwrap();
            serial = v.serial_no;
        }
        let airspy = Airspy::from_serial(serial);
        assert!(airspy.is_ok());
        let _ = exit();
    }

    #[test]
    #[ignore = "requires an attached Airspy device; run with RUST_TEST_THREADS=1"]
    fn test_get_sample_rates() {
        let _ = init();
        let mut airspy = Airspy::new().unwrap();
        let rates = airspy.get_sample_rates();
        assert!(rates.is_ok());
        let rates = rates.unwrap();
        assert!(rates.len() == 2);
        assert!(rates.contains(&2500000));
        assert!(rates.contains(&10000000));
        let _ = exit();
    }

    #[test]
    #[ignore = "requires an attached Airspy device; run with RUST_TEST_THREADS=1"]
    fn test_set_sample_rate() {
        let _ = init();
        let mut airspy = Airspy::new().unwrap();
        assert!(airspy.set_sample_rate(10_000_000).is_ok());
    }

    #[test]
    #[ignore = "requires an attached Airspy device; run with RUST_TEST_THREADS=1"]
    fn test_get_board() {
        let _ = init();
        let mut airspy = Airspy::new().unwrap();
        assert!(airspy.get_board_id().unwrap() == "AIRSPY");
    }

    #[test]
    #[ignore = "requires an attached Airspy device; run with RUST_TEST_THREADS=1"]
    fn test_get_version() {
        let _ = init();
        let mut airspy = Airspy::new().unwrap();
        assert!(airspy.get_version().is_ok());
        // Don't check this value as it is hardware-dependent.
    }

    #[test]
    #[ignore = "requires an attached Airspy device; run with RUST_TEST_THREADS=1"]
    fn test_get_partid_serial() {
        let _ = init();
        let mut airspy = Airspy::new().unwrap();
        assert!(airspy.get_partid_serial().is_ok());
        // Don't check these values as they are hardware-dependent.
    }

    #[test]
    #[ignore = "requires an attached Airspy device; run with RUST_TEST_THREADS=1"]
    fn test_set_freq() {
        let _ = init();
        let mut airspy = Airspy::new().unwrap();
        assert!(airspy.set_freq(434000000).is_ok());
    }

    #[test]
    #[ignore = "requires an attached Airspy device; run with RUST_TEST_THREADS=1"]
    fn test_set_lna_gain() {
        let _ = init();
        let mut airspy = Airspy::new().unwrap();
        assert!(airspy.set_lna_gain(7).is_ok());
    }

    #[test]
    #[ignore = "requires an attached Airspy device; run with RUST_TEST_THREADS=1"]
    fn test_set_mixer_gain() {
        let _ = init();
        let mut airspy = Airspy::new().unwrap();
        assert!(airspy.set_mixer_gain(7).is_ok());
    }

    #[test]
    #[ignore = "requires an attached Airspy device; run with RUST_TEST_THREADS=1"]
    fn test_set_vga_gain() {
        let _ = init();
        let mut airspy = Airspy::new().unwrap();
        assert!(airspy.set_vga_gain(7).is_ok());
    }

    #[test]
    #[ignore = "requires an attached Airspy device; run with RUST_TEST_THREADS=1"]
    fn test_set_lna_agc() {
        let _ = init();
        let mut airspy = Airspy::new().unwrap();
        assert!(airspy.set_lna_agc(true).is_ok());
    }

    #[test]
    #[ignore = "requires an attached Airspy device; run with RUST_TEST_THREADS=1"]
    fn test_set_mixer_agc() {
        let _ = init();
        let mut airspy = Airspy::new().unwrap();
        assert!(airspy.set_mixer_agc(true).is_ok());
    }

    #[test]
    #[ignore = "requires an attached Airspy device; run with RUST_TEST_THREADS=1"]
    fn test_set_rf_bias() {
        let _ = init();
        let mut airspy = Airspy::new().unwrap();
        assert!(airspy.set_rf_bias(true).is_ok());
    }
}
