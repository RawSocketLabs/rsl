use crate::rx::RxState;
use crate::{Error, Result, ffi};
use rfus::SampleRateSps;
use std::ffi::{CStr, c_int};
use std::fmt;
use std::ptr::{self, NonNull};
use std::sync::Arc;

/// Firmware version buffer size; libairspy's own limit is smaller.
const VERSION_LEN: u8 = u8::MAX;

/// An open Airspy device.
pub struct Airspy {
    /// Live libairspy handle: set only by a successful open, closed (or,
    /// with orphaned threads, leaked) only in `Drop`.
    ptr: NonNull<ffi::AirspyDevice>,
    /// The running session's callback state, while libairspy may call back
    /// into it. Only `end_session` takes it.
    pub(crate) session: Option<Arc<RxState>>,
    /// Set when a stop may have left libairspy's threads running (see
    /// `ffi::airspy_stop_rx`), so `start_rx` refuses and `Drop` leaks the
    /// handle.
    pub(crate) threads_orphaned: bool,
}

impl fmt::Debug for Airspy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Airspy").finish_non_exhaustive()
    }
}

// SAFETY: libairspy handles are not tied to the thread that opened them, and
// every call goes through `&mut self`, so moving the device between threads
// never makes two calls race. The callback state is an `Arc` of a `Sync`
// type. `Airspy` stays `!Sync`.
unsafe impl Send for Airspy {}

/// End any session and close the device, or leak the handle if libairspy's
/// threads may still be running.
impl Drop for Airspy {
    fn drop(&mut self) {
        // A dropped Receiver's session is still running.
        let _ = self.end_session();
        // Closing would free the device under libairspy's orphaned threads.
        if self.threads_orphaned {
            return;
        }
        // SAFETY: `ptr` is live (field invariant) and unused after this. Any
        // session was stopped and joined above.
        unsafe {
            ffi::airspy_close(self.ptr.as_ptr());
        }
    }
}

/// An Airspy board's part ID and serial number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Identity {
    /// The two part ID words, as libairspy reports them.
    pub part_id: [u32; 2],
    /// The 64-bit serial number `open_serial` accepts.
    pub serial: u64,
}

impl Airspy {
    /// Try to open the next available Airspy device.
    ///
    /// # Errors
    ///
    /// [`Error::NotFound`] if no unopened Airspy is attached; otherwise
    /// libairspy's status.
    pub fn open() -> Result<Self> {
        let mut device = ptr::null_mut();
        let status = ffi::airspy_open(&mut device);
        Self::from_open(status, device)
    }

    /// Try to open a specific Airspy device by serial number.
    ///
    /// # Errors
    ///
    /// [`Error::NotFound`] if no unopened Airspy has that serial; otherwise
    /// libairspy's status.
    pub fn open_serial(serial_number: u64) -> Result<Self> {
        let mut device = ptr::null_mut();
        let status = ffi::airspy_open_sn(&mut device, serial_number);
        Self::from_open(status, device)
    }

    /// Take ownership of the handle a libairspy open call returned.
    fn from_open(status: c_int, device: *mut ffi::AirspyDevice) -> Result<Self> {
        Error::check(status)?;
        // libairspy reports success only alongside a handle.
        let ptr = NonNull::new(device).ok_or(Error::Other)?;
        Ok(Self {
            ptr,
            session: None,
            threads_orphaned: false,
        })
    }

    /// Borrow the live handle for a libairspy call.
    pub(crate) fn handle(&mut self) -> &mut ffi::AirspyDevice {
        // SAFETY: `ptr` is live (field invariant). The type is zero-sized and
        // aligned to 1, so the borrow covers none of libairspy's bytes.
        unsafe { self.ptr.as_mut() }
    }

    /// Get available sample rates for this Airspy.
    ///
    /// # Errors
    ///
    /// libairspy's status if either query fails.
    pub fn sample_rates(&mut self) -> Result<Vec<SampleRateSps>> {
        let mut count: u32 = 0;
        // SAFETY: with `len == 0`, libairspy writes only the count, to `count`.
        Error::check(unsafe { ffi::airspy_get_samplerates(self.handle(), &raw mut count, 0) })?;
        // An empty `rates` would hand libairspy a dangling pointer to write to.
        if count == 0 {
            return Ok(Vec::new());
        }
        let mut rates = vec![0; count as usize];
        // SAFETY: `rates` holds exactly `count` writable elements.
        Error::check(unsafe {
            ffi::airspy_get_samplerates(self.handle(), rates.as_mut_ptr(), count)
        })?;
        Ok(rates.into_iter().map(SampleRateSps::new).collect())
    }

    /// Set this Airspy's sample rate to a specific rate.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidParameter`] if the device does not advertise
    /// `target_rate`; otherwise libairspy's status.
    pub fn set_sample_rate(&mut self, target_rate: SampleRateSps) -> Result<()> {
        let index = self
            .sample_rates()?
            .iter()
            .zip(0u32..)
            .find_map(|(&rate, index)| (rate == target_rate).then_some(index))
            .ok_or(Error::InvalidParameter)?;
        Error::check(ffi::airspy_set_samplerate(self.handle(), index))
    }

    /// Read the board's name as libairspy reports it, such as `AIRSPY`.
    ///
    /// # Errors
    ///
    /// libairspy's status if the board ID read fails.
    pub fn board_name(&mut self) -> Result<&'static str> {
        let mut id: u8 = 0;
        Error::check(ffi::airspy_board_id_read(self.handle(), &mut id))?;
        let name = ffi::airspy_board_id_name(id.into());
        // SAFETY: airspy_board_id_name returns a string literal for every id,
        // including unknown ones.
        let name: &'static CStr = unsafe { CStr::from_ptr(name) };
        Ok(name.to_str().unwrap_or("unknown"))
    }

    /// Read the firmware version string.
    ///
    /// # Errors
    ///
    /// libairspy's status, or [`Error::Other`] if the firmware string is not
    /// NUL-terminated.
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
    ///
    /// # Errors
    ///
    /// libairspy's status if the read fails.
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
}

/// The tuning setters, which stay valid while streaming, so [`Airspy`] and
/// [`Receiver`](crate::Receiver) share them. Expands inside any impl with a
/// `handle(&mut self) -> &mut ffi::AirspyDevice`; every other path is `$crate`.
macro_rules! tuning_setters {
    () => {
        /// Set the centre frequency; the tuner covers 24 MHz to 1.75 GHz.
        ///
        /// Neither this crate nor libairspy checks that range: any `u32` is
        /// sent to the device. libairspy takes whole hertz as a `u32`, so a
        /// frequency above `u32::MAX` Hz is [`Error::InvalidParameter`] before
        /// any call.
        pub fn set_frequency(&mut self, freq: $crate::FrequencyHz) -> $crate::Result<()> {
            let hz = u32::try_from(freq.hz()).map_err(|_| $crate::Error::InvalidParameter)?;
            $crate::Error::check($crate::ffi::airspy_set_freq(self.handle(), hz))
        }

        /// Set the LNA gain step index, 0 to 14; libairspy clamps larger values.
        pub fn set_lna_gain(&mut self, gain: u8) -> $crate::Result<()> {
            $crate::Error::check($crate::ffi::airspy_set_lna_gain(self.handle(), gain))
        }

        /// Set the mixer gain step index, 0 to 15; libairspy clamps larger values.
        pub fn set_mixer_gain(&mut self, gain: u8) -> $crate::Result<()> {
            $crate::Error::check($crate::ffi::airspy_set_mixer_gain(self.handle(), gain))
        }

        /// Set the VGA gain step index, 0 to 15; libairspy clamps larger values.
        pub fn set_vga_gain(&mut self, gain: u8) -> $crate::Result<()> {
            $crate::Error::check($crate::ffi::airspy_set_vga_gain(self.handle(), gain))
        }

        /// Enable or disable the LNA AGC.
        pub fn set_lna_agc(&mut self, enable: bool) -> $crate::Result<()> {
            $crate::Error::check($crate::ffi::airspy_set_lna_agc(
                self.handle(),
                u8::from(enable),
            ))
        }

        /// Enable or disable the mixer AGC.
        pub fn set_mixer_agc(&mut self, enable: bool) -> $crate::Result<()> {
            $crate::Error::check($crate::ffi::airspy_set_mixer_agc(
                self.handle(),
                u8::from(enable),
            ))
        }

        /// Enable or disable the RF bias voltage.
        pub fn set_rf_bias(&mut self, enable: bool) -> $crate::Result<()> {
            $crate::Error::check($crate::ffi::airspy_set_rf_bias(
                self.handle(),
                u8::from(enable),
            ))
        }
    };
}
pub(crate) use tuning_setters;

/// Tuning: each setter forwards one value and returns libairspy's status.
#[expect(
    clippy::missing_errors_doc,
    reason = "each returns libairspy's status unchanged; Error documents the variants"
)]
impl Airspy {
    tuning_setters!();
}

#[cfg(test)]
mod tests {
    use super::*;
    use rfus::FrequencyHz;

    #[test]
    fn device_moves_between_threads() {
        fn assert_send<T: Send>() {}
        assert_send::<Airspy>();
    }

    #[test]
    #[ignore = "requires an attached Airspy device; run with RUST_TEST_THREADS=1"]
    fn discovers_device_and_available_rates() {
        let mut device = Airspy::open().unwrap();
        assert_ne!(device.board_name().unwrap(), "");
        assert_ne!(device.firmware_version().unwrap(), "");
        let rates = device.sample_rates().unwrap();
        assert_ne!(rates.len(), 0);
        assert!(rates.iter().all(|rate| rate.sps() > 0));
    }

    #[test]
    #[ignore = "requires an attached Airspy device; run with RUST_TEST_THREADS=1"]
    fn configures_device_with_advertised_rate() {
        let mut device = Airspy::open().unwrap();
        let rates = device.sample_rates().unwrap();
        // The last rate, so an index lookup stuck at 0 cannot pass.
        let rate = *rates.last().expect("device must advertise a sample rate");
        device.set_sample_rate(rate).unwrap();
        device.set_frequency(FrequencyHz::new(434_000_000)).unwrap();
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
