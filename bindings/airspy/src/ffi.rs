// Rust binding to libairspy
// Copyright 2015 Adam Greig <adam@adamgreig.com>
// Licensed under the MIT License.

use std::ffi::{c_char, c_int, c_void};

/// Opaque libairspy device handle.
#[repr(C)]
pub struct AirspyDevice {
    _private: [u8; 0],
}

#[repr(C)]
pub struct airspy_lib_version_t {
    pub major_version: u32,
    pub minor_version: u32,
    pub revision: u32,
}

#[repr(C)]
pub struct airspy_read_partid_serialno_t {
    pub part_id: [u32; 2],
    pub serial_no: [u32; 4],
}

/// Common transfer prefix in libairspy 1.0.6 and later; never allocated or passed by value.
/// The version-dependent tail is deliberately inaccessible.
#[repr(C)]
pub struct airspy_transfer_t {
    pub device: *mut AirspyDevice,
    pub ctx: *mut c_void,
    pub samples: *const c_void,
    pub sample_count: c_int,
}

#[link(name = "airspy")]
extern "C" {
    pub fn airspy_lib_version(version: &mut airspy_lib_version_t);
    pub fn airspy_open_sn(device: *mut *mut AirspyDevice, serial_number: u64) -> c_int;
    pub fn airspy_open(device: *mut *mut AirspyDevice) -> c_int;
    pub fn airspy_close(device: *mut AirspyDevice) -> c_int;
    pub fn airspy_get_samplerates(device: *mut AirspyDevice, buffer: *mut u32, len: u32) -> c_int;
    pub fn airspy_set_samplerate(device: *mut AirspyDevice, samplerate: u32) -> c_int;
    pub fn airspy_start_rx(
        device: *mut AirspyDevice,
        callback: unsafe extern "C" fn(*mut airspy_transfer_t) -> c_int,
        rx_ctx: *mut c_void,
    ) -> c_int;
    pub fn airspy_stop_rx(device: *mut AirspyDevice) -> c_int;
    pub fn airspy_is_streaming(device: *mut AirspyDevice) -> c_int;
    pub fn airspy_board_id_read(device: *mut AirspyDevice, value: *mut u8) -> c_int;
    pub fn airspy_version_string_read(
        device: *mut AirspyDevice,
        version: *mut c_char,
        length: u8,
    ) -> c_int;
    pub fn airspy_board_partid_serialno_read(
        device: *mut AirspyDevice,
        read_partid_serialno: *mut airspy_read_partid_serialno_t,
    ) -> c_int;
    pub fn airspy_set_sample_type(device: *mut AirspyDevice, sample_type: c_int) -> c_int;
    pub fn airspy_set_freq(device: *mut AirspyDevice, freq_hz: u32) -> c_int;
    pub fn airspy_set_lna_gain(device: *mut AirspyDevice, value: u8) -> c_int;
    pub fn airspy_set_mixer_gain(device: *mut AirspyDevice, value: u8) -> c_int;
    pub fn airspy_set_vga_gain(device: *mut AirspyDevice, value: u8) -> c_int;
    pub fn airspy_set_lna_agc(device: *mut AirspyDevice, value: u8) -> c_int;
    pub fn airspy_set_mixer_agc(device: *mut AirspyDevice, value: u8) -> c_int;
    pub fn airspy_set_rf_bias(device: *mut AirspyDevice, value: u8) -> c_int;
    pub fn airspy_board_id_name(board_id: c_int) -> *const c_char;
}
