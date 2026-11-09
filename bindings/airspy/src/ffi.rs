// Rust binding to libairspy
// Copyright 2015 Adam Greig <adam@adamgreig.com>
// Licensed under the MIT License.

//! libairspy declarations (`airspy.h`, libairspy 1.0.6 and later).
//!
//! Items marked `safe` have no precondition a Rust caller can break: they take
//! a live device handle as `&mut AirspyDevice`, plain values, or Rust-owned
//! out-parameters. The remaining items keep an `unsafe` contract stated on
//! each declaration.

use std::ffi::{c_char, c_int, c_void};
use std::marker::{PhantomData, PhantomPinned};

/// Opaque libairspy device, only ever reached through the handle `airspy_open`
/// returned.
///
/// Zero-sized, so a `&mut AirspyDevice` asserts nothing about the C-owned
/// bytes that libairspy's own threads also use; the marker keeps it `!Send`,
/// `!Sync` and `!Unpin`.
#[repr(C)]
pub(crate) struct AirspyDevice {
    _data: [u8; 0],
    _marker: PhantomData<(*mut u8, PhantomPinned)>,
}

/// `airspy_lib_version_t`: the linked library's version.
#[repr(C)]
pub(crate) struct airspy_lib_version_t {
    pub(crate) major_version: u32,
    pub(crate) minor_version: u32,
    pub(crate) revision: u32,
}

/// `airspy_read_partid_serialno_t`: the board's part ID and serial words.
#[repr(C)]
pub(crate) struct airspy_read_partid_serialno_t {
    pub(crate) part_id: [u32; 2],
    pub(crate) serial_no: [u32; 4],
}

/// Common transfer prefix in libairspy 1.0.6 and later; never allocated or
/// passed by value. The version-dependent tail is deliberately inaccessible.
#[repr(C)]
pub(crate) struct airspy_transfer_t {
    pub(crate) device: *mut AirspyDevice,
    pub(crate) ctx: *mut c_void,
    pub(crate) samples: *const c_void,
    pub(crate) sample_count: c_int,
}

/// `enum airspy_sample_type`, limited to the format this crate receives.
///
/// A Rust enum here means no out-of-range value can reach libairspy.
#[repr(C)]
pub(crate) enum SampleType {
    /// `AIRSPY_SAMPLE_FLOAT32_IQ`: interleaved `f32` I/Q pairs.
    Float32Iq = 0,
}

// `enum airspy_error` (airspy.h): every call's status; only success is zero.
pub(crate) const AIRSPY_SUCCESS: c_int = 0;
pub(crate) const AIRSPY_ERROR_INVALID_PARAM: c_int = -2;
pub(crate) const AIRSPY_ERROR_NOT_FOUND: c_int = -5;
pub(crate) const AIRSPY_ERROR_BUSY: c_int = -6;
pub(crate) const AIRSPY_ERROR_NO_MEM: c_int = -11;
pub(crate) const AIRSPY_ERROR_UNSUPPORTED: c_int = -12;
pub(crate) const AIRSPY_ERROR_LIBUSB: c_int = -1000;
pub(crate) const AIRSPY_ERROR_THREAD: c_int = -1001;
pub(crate) const AIRSPY_ERROR_STREAMING_THREAD_ERR: c_int = -1002;
pub(crate) const AIRSPY_ERROR_STREAMING_STOPPED: c_int = -1003;
pub(crate) const AIRSPY_ERROR_OTHER: c_int = -9999;

/// `AIRSPY_TRUE`, as returned by `airspy_is_streaming`.
pub(crate) const AIRSPY_TRUE: c_int = 1;

/// Callback return that keeps libairspy streaming.
pub(crate) const RX_CONTINUE: c_int = 0;

/// Callback return that makes libairspy stop delivering samples.
pub(crate) const RX_STOP: c_int = 1;

/// Rust stand-ins for the session calls, since Miri cannot call C.
#[cfg(all(miri, test))]
pub(crate) mod fake;
#[cfg(all(miri, test))]
pub(crate) use fake::{
    airspy_close, airspy_is_streaming, airspy_lib_version, airspy_open, airspy_set_sample_type,
    airspy_start_rx, airspy_stop_rx,
};

// SAFETY: every signature matches airspy.h for libairspy 1.0.6 and later.
// The `safe` items need only a live handle. `AirspyDevice` is never
// constructed, and `Airspy::handle` is the only source of a
// `&mut AirspyDevice`, so every such reference names a live handle. Their
// value arguments only reach USB control requests or are clamped (the gains),
// never index host memory, and libairspy writes only to the out-parameters
// it is given.
#[link(name = "airspy")]
unsafe extern "C" {
    #[cfg(not(all(miri, test)))]
    pub(crate) safe fn airspy_lib_version(version: &mut airspy_lib_version_t);
    #[cfg(not(all(miri, test)))]
    pub(crate) safe fn airspy_open(device: &mut *mut AirspyDevice) -> c_int;
    pub(crate) safe fn airspy_open_sn(device: &mut *mut AirspyDevice, serial_number: u64) -> c_int;
    /// Runs `airspy_stop_rx` (see its caveat), then frees `device`.
    ///
    /// Callers must pass a live handle and never use it again.
    #[cfg(not(all(miri, test)))]
    pub(crate) fn airspy_close(device: *mut AirspyDevice) -> c_int;
    /// With `len == 0`, writes only the number of supported rates to
    /// `buffer[0]`; otherwise writes up to `len` rates.
    ///
    /// Callers must provide `max(len, 1)` writable elements.
    pub(crate) fn airspy_get_samplerates(
        device: &mut AirspyDevice,
        buffer: *mut u32,
        len: u32,
    ) -> c_int;
    /// Takes an index into the supported rates; 1.0.6 accepts nothing else.
    pub(crate) safe fn airspy_set_samplerate(device: &mut AirspyDevice, samplerate: u32) -> c_int;
    /// Calls `callback` from libairspy's threads with `rx_ctx` until stopped.
    ///
    /// Callers must keep whatever `rx_ctx` points to alive and valid for
    /// `callback` until `airspy_stop_rx` or `airspy_close` has returned.
    ///
    /// In releases 1.0.6 to 1.0.10, if submitting the USB transfers or
    /// creating a thread fails, it returns with `streaming` set but threads
    /// missing, and the next stop or close joins a thread that was never
    /// created. That is undefined behaviour inside libairspy, and no caller
    /// can avoid it; libairspy's master joins only threads it started.
    #[cfg(not(all(miri, test)))]
    pub(crate) fn airspy_start_rx(
        device: &mut AirspyDevice,
        callback: unsafe extern "C" fn(*mut airspy_transfer_t) -> c_int,
        rx_ctx: *mut c_void,
    ) -> c_int;
    /// Joins the streaming threads before returning, whatever it reports,
    /// unless libairspy already ended the stream itself after a USB failure:
    /// releases 1.0.6 to 1.0.10 then skip the join, so `Receiver` leaks its
    /// state on any release before 1.0.12, which always joins. In those
    /// releases a callback's stop return does not trigger this, because it only sets `stop_requested`; master clears `streaming`
    /// instead, which is harmless there because its stop always joins.
    ///
    /// After a skipped join the consumer thread lives on inside libairspy, so
    /// restarting (a second consumer) or closing (freeing its condvar and
    /// device) is undefined behaviour there. `Airspy` refuses both once a stop
    /// finds the stream already ended, on a best-effort basis.
    #[cfg(not(all(miri, test)))]
    pub(crate) safe fn airspy_stop_rx(device: &mut AirspyDevice) -> c_int;
    #[cfg(not(all(miri, test)))]
    pub(crate) safe fn airspy_is_streaming(device: &mut AirspyDevice) -> c_int;
    pub(crate) safe fn airspy_board_id_read(device: &mut AirspyDevice, value: &mut u8) -> c_int;
    /// Writes at most `length` bytes to `version`.
    ///
    /// Callers must provide `length` writable bytes.
    pub(crate) fn airspy_version_string_read(
        device: &mut AirspyDevice,
        version: *mut c_char,
        length: u8,
    ) -> c_int;
    pub(crate) safe fn airspy_board_partid_serialno_read(
        device: &mut AirspyDevice,
        read_partid_serialno: &mut airspy_read_partid_serialno_t,
    ) -> c_int;
    #[cfg(not(all(miri, test)))]
    pub(crate) safe fn airspy_set_sample_type(
        device: &mut AirspyDevice,
        sample_type: SampleType,
    ) -> c_int;
    pub(crate) safe fn airspy_set_freq(device: &mut AirspyDevice, freq_hz: u32) -> c_int;
    pub(crate) safe fn airspy_set_lna_gain(device: &mut AirspyDevice, value: u8) -> c_int;
    pub(crate) safe fn airspy_set_mixer_gain(device: &mut AirspyDevice, value: u8) -> c_int;
    pub(crate) safe fn airspy_set_vga_gain(device: &mut AirspyDevice, value: u8) -> c_int;
    pub(crate) safe fn airspy_set_lna_agc(device: &mut AirspyDevice, value: u8) -> c_int;
    pub(crate) safe fn airspy_set_mixer_agc(device: &mut AirspyDevice, value: u8) -> c_int;
    pub(crate) safe fn airspy_set_rf_bias(device: &mut AirspyDevice, value: u8) -> c_int;
    /// Returns a string literal for any id, including unknown ones.
    pub(crate) safe fn airspy_board_id_name(board_id: c_int) -> *const c_char;
}
