//! libusdr's oscillators: the fixed-point sine/cosine (`xdsp/trig.c`), the NCO frequency
//! shifter (`xdsp/nco.c`) and the floating-point sine/cosine tables, tones and chirps
//! (`xdsp/sincos_functions.c`, generic kernels selected with `OPT_GENERIC`).
//!
//! `nco_shift` has only an SSSE3 body, under `#ifdef __SSSE3__`; the default oracle build
//! passes no `-mssse3`, so it compiles the stub that returns 0. [`nco_compiled`] tells the
//! two apart. Building with `CFLAGS=-mssse3` compiles the kernel.

use std::ffi::{c_char, c_uint, c_void};
use std::ptr;

/// `conv_function_t`.
type ConvFn = unsafe extern "C" fn(*const *const c_void, c_uint, *const *mut c_void, c_uint);
/// `sincos_i16_interleaved_ctrl_function_t`.
type ToneFn = unsafe extern "C" fn(*mut i32, i32, i16, bool, bool, *mut i16, c_uint);
/// `sincos_i16_interleaved_chirp_function_t`.
type ChirpFn =
    unsafe extern "C" fn(*mut i32, *mut i32, *mut i32, i32, i16, bool, bool, *mut i16, c_uint);

/// `OPT_GENERIC`: select the generic kernel.
const OPT_GENERIC: c_uint = 0;

unsafe extern "C" {
    fn isincos_generic(ph: *const i16, psin: *mut i16, pcos: *mut i16);
    fn nco_shift(
        inphase: i32,
        delta: i32,
        iqbuf: *const i16,
        csamples: c_uint,
        out: *mut i16,
    ) -> i32;
    fn get_wvlt_sincos_i16_c(cpu_cap: c_uint, sfunc: *mut *const c_char) -> Option<ConvFn>;
    fn get_wvlt_sincos_i16_interleaved_ctrl_c(
        cpu_cap: c_uint,
        sfunc: *mut *const c_char,
    ) -> Option<ToneFn>;
    fn get_wvlt_sincos_i16_interleaved_chirp_c(
        cpu_cap: c_uint,
        sfunc: *mut *const c_char,
    ) -> Option<ChirpFn>;
}

/// `isincos_generic(&phase, &sin, &cos)`: `(sin, cos)`.
#[must_use]
pub fn isincos(phase: i16) -> (i16, i16) {
    let (mut sin, mut cos) = (0, 0);
    // SAFETY: three valid, distinct `int16_t` locations for the call.
    unsafe { isincos_generic(&raw const phase, &raw mut sin, &raw mut cos) };
    (sin, cos)
}

/// Whether `nco_shift` is the SSSE3 kernel rather than the stub: the kernel returns the
/// phase advanced past eight samples (9 from phase 1, step 1); the stub returns 0.
#[must_use]
pub fn nco_compiled() -> bool {
    nco(1, 1, &[0; 16]).1 == 9
}

/// `nco_shift(phase, delta, input, input.len() / 2, out)`: the shifted pairs and the
/// returned phase.
///
/// The SSSE3 kernel works in blocks of eight samples, reading and writing past a count that
/// is not a multiple of eight; both buffers are zero-padded to whole blocks, and only the
/// requested samples are returned. The stub writes nothing, leaving zeros.
///
/// # Panics
///
/// If `input` holds an odd count or more samples than `unsigned`.
#[must_use]
pub fn nco(phase: i32, delta: i32, input: &[i16]) -> (Vec<i16>, i32) {
    assert!(input.len() % 2 == 0, "whole I/Q pairs");
    let samples = input.len() / 2;
    let padded_len = 2 * samples.next_multiple_of(8);
    let mut padded = input.to_vec();
    padded.resize(padded_len, 0);
    let mut out = vec![0; padded_len];
    let count = c_uint::try_from(samples).expect("fits unsigned");
    // SAFETY: both buffers hold whole eight-sample blocks covering `count` samples, the
    // most the kernel reads or writes; they do not overlap and outlive the call.
    let next = unsafe { nco_shift(phase, delta, padded.as_ptr(), count, out.as_mut_ptr()) };
    out.truncate(input.len());
    (out, next)
}

/// The generic `wvlt_sincos_i16` over `phases`: `(sin, cos)`.
///
/// # Panics
///
/// If the byte count exceeds `unsigned`.
#[must_use]
pub fn sin_cos_i16(phases: &[i16]) -> (Vec<i16>, Vec<i16>) {
    let mut sin = vec![0; phases.len()];
    let mut cos = vec![0; phases.len()];
    let bytes = c_uint::try_from(2 * phases.len()).expect("fits unsigned");
    // SAFETY: a pure selection.
    let kernel = unsafe { get_wvlt_sincos_i16_c(OPT_GENERIC, ptr::null_mut()) }
        .expect("libusdr has a generic sincos kernel");
    let input = [phases.as_ptr().cast::<c_void>()];
    let outputs = [sin.as_mut_ptr().cast::<c_void>(), cos.as_mut_ptr().cast()];
    // SAFETY: one input of `bytes` bytes and two distinct outputs of `bytes` bytes each,
    // as `wvlt_sincos_i16` passes them; all outlive the call.
    unsafe { kernel(input.as_ptr(), bytes, outputs.as_ptr(), bytes) };
    (sin, cos)
}

/// The generic `wvlt_sincos_i16_interleaved_ctrl`: `iters` interleaved `(sin, cos)`
/// pairs, updating `phase`.
///
/// # Panics
///
/// If `iters` exceeds `unsigned`.
#[must_use]
pub fn tone(
    phase: &mut i32,
    delta: i32,
    gain: i16,
    invert: (bool, bool),
    iters: usize,
) -> Vec<i16> {
    let mut out = vec![0; 2 * iters];
    let count = c_uint::try_from(iters).expect("fits unsigned");
    // SAFETY: a pure selection.
    let kernel = unsafe { get_wvlt_sincos_i16_interleaved_ctrl_c(OPT_GENERIC, ptr::null_mut()) }
        .expect("libusdr has a generic tone kernel");
    // SAFETY: `phase` is a valid `int32_t` and `out` holds the `2 * iters` values written.
    unsafe {
        kernel(
            phase,
            delta,
            gain,
            invert.0,
            invert.1,
            out.as_mut_ptr(),
            count,
        );
    };
    out
}

/// The generic `wvlt_sincos_i16_interleaved_chirp`: `iters` interleaved `(sin, cos)`
/// pairs, updating `phase` and `delta`; `bounds` is `delta_phase[2]`.
///
/// # Panics
///
/// If `steps` is zero (the C divides by it) or `iters` exceeds `unsigned`.
#[must_use]
pub fn chirp(
    phase: &mut i32,
    delta: &mut i32,
    bounds: [i32; 2],
    steps: i32,
    gain: i16,
    invert: (bool, bool),
    iters: usize,
) -> Vec<i16> {
    assert_ne!(steps, 0, "libusdr divides by the step count");
    let mut bounds = bounds;
    let mut out = vec![0; 2 * iters];
    let count = c_uint::try_from(iters).expect("fits unsigned");
    // SAFETY: a pure selection.
    let kernel = unsafe { get_wvlt_sincos_i16_interleaved_chirp_c(OPT_GENERIC, ptr::null_mut()) }
        .expect("libusdr has a generic chirp kernel");
    // SAFETY: `phase`, `delta` and the two `bounds` are valid, distinct `int32_t`s, and
    // `out` holds the `2 * iters` values written.
    unsafe {
        kernel(
            phase,
            delta,
            bounds.as_mut_ptr(),
            steps,
            gain,
            invert.0,
            invert.1,
            out.as_mut_ptr(),
            count,
        );
    };
    out
}
