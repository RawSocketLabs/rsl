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

/// `AIRSPY_TRUE`, as returned by `airspy_is_streaming`.
pub(crate) const AIRSPY_TRUE: c_int = 1;

/// Callback return that keeps libairspy streaming.
pub(crate) const RX_CONTINUE: c_int = 0;

/// Callback return that makes libairspy stop delivering samples.
pub(crate) const RX_STOP: c_int = 1;

// SAFETY: every signature matches airspy.h for libairspy 1.0.6 and later.
// The `safe` items need only a live handle, which `&mut AirspyDevice`
// guarantees (`Airspy::handle` is its sole source). libairspy validates or
// clamps their value arguments and writes only to the out-parameters it is
// given.
#[link(name = "airspy")]
unsafe extern "C" {
    pub(crate) safe fn airspy_lib_version(version: &mut airspy_lib_version_t);
    pub(crate) safe fn airspy_open(device: &mut *mut AirspyDevice) -> c_int;
    pub(crate) safe fn airspy_open_sn(device: &mut *mut AirspyDevice, serial_number: u64) -> c_int;
    /// Stops RX, joins the streaming threads, then frees `device`.
    ///
    /// Callers must pass a live handle and never use it again.
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
    /// Takes an index into the supported rates, or a rate in hertz.
    pub(crate) safe fn airspy_set_samplerate(device: &mut AirspyDevice, samplerate: u32) -> c_int;
    /// Calls `callback` from libairspy's threads with `rx_ctx` until stopped.
    ///
    /// Callers must keep whatever `rx_ctx` points to alive and valid for
    /// `callback` until `airspy_stop_rx` or `airspy_close` has returned.
    pub(crate) fn airspy_start_rx(
        device: &mut AirspyDevice,
        callback: unsafe extern "C" fn(*mut airspy_transfer_t) -> c_int,
        rx_ctx: *mut c_void,
    ) -> c_int;
    /// Joins the streaming threads before returning, whatever it reports.
    pub(crate) safe fn airspy_stop_rx(device: &mut AirspyDevice) -> c_int;
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
    /// Returns a NUL-terminated entry of a static name table for any id,
    /// including unknown ones.
    pub(crate) safe fn airspy_board_id_name(board_id: c_int) -> *const c_char;
}
