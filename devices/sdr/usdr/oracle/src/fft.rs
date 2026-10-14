//! libusdr's FFT windowing (`xdsp/fft_window_functions.c`) and spectrum accumulation
//! (`xdsp/fftad_functions.c`), called through their own dispatchers.
//!
//! The vendored build defines no `WVLT_ARCH_*`, so the dispatchers return the generic
//! (scalar) kernels: the reference `usdr::dsp::{window, fftad}` port. Complex samples
//! are `[re, im]` pairs, libusdr's `wvlt_fftwf_complex` and `wvlt_fftwi16_complex`.

use std::ffi::{c_char, c_uint};
use std::ptr;

/// `generic_opts_t::OPT_GENERIC`.
const OPT_GENERIC: c_uint = 0;

/// `fft_acc_t`.
#[repr(C)]
struct FftAcc {
    /// Per-bin mantissas or sums.
    f_mant: *mut f32,
    /// Per-bin exponents.
    f_pwr: *mut i32,
    /// Power floor.
    mine: f32,
}

/// `fft_window_cf32_function_t`.
type WindowCf32Fn = unsafe extern "C" fn(*mut [f32; 2], c_uint, *mut f32, *mut [f32; 2]);
/// `fft_window_ci16_cf32_function_t`.
type WindowCi16Fn = unsafe extern "C" fn(*mut [i16; 2], c_uint, *mut f32, *mut [f32; 2]);
/// `fftad_init_function_t`.
type InitFn = unsafe extern "C" fn(*mut FftAcc, c_uint);
/// `fftad_add_function_t`.
type AddFn = unsafe extern "C" fn(*mut FftAcc, *mut [f32; 2], c_uint);
/// `fftad_add_hwi16_function_t`.
type AddHwFn = unsafe extern "C" fn(*mut FftAcc, *mut u16, c_uint);
/// `fftad_norm_function_t`.
type NormFn = unsafe extern "C" fn(*mut FftAcc, c_uint, f32, f32, *mut f32);

unsafe extern "C" {
    fn fft_window_cf32_c(cpu_cap: c_uint, sfunc: *mut *const c_char) -> WindowCf32Fn;
    fn fft_window_ci16_cf32_c(cpu_cap: c_uint, sfunc: *mut *const c_char) -> WindowCi16Fn;
    fn fftad_init_c(cpu_cap: c_uint, sfunc: *mut *const c_char) -> InitFn;
    fn fftad_add_c(cpu_cap: c_uint, sfunc: *mut *const c_char) -> AddFn;
    fn fftad_norm_c(cpu_cap: c_uint, sfunc: *mut *const c_char) -> NormFn;
    fn fftad_init_hwi16_c(cpu_cap: c_uint, sfunc: *mut *const c_char) -> InitFn;
    fn fftad_add_hwi16_c(cpu_cap: c_uint, sfunc: *mut *const c_char) -> AddHwFn;
    fn fftad_norm_hwi16_c(cpu_cap: c_uint, sfunc: *mut *const c_char) -> NormFn;
}

/// `len` as libusdr's `unsigned` size.
fn size(len: usize) -> c_uint {
    c_uint::try_from(len).expect("sizes fit unsigned")
}

/// `fft_window_cf32`: windows `input` by `window` into a new buffer.
///
/// # Panics
///
/// If the lengths differ or exceed `unsigned`.
#[must_use]
pub fn window_cf32(input: &[[f32; 2]], window: &[f32]) -> Vec<[f32; 2]> {
    assert_eq!(input.len(), window.len(), "one coefficient per sample");
    let mut input = input.to_vec();
    let mut window = window.to_vec();
    let mut out = vec![[f32::NAN; 2]; input.len()];
    // SAFETY: the selector only picks a function pointer.
    let kernel = unsafe { fft_window_cf32_c(OPT_GENERIC, ptr::null_mut()) };
    // SAFETY: each buffer holds the `fftsz` elements the kernel reads or writes, and they
    // do not overlap (the kernel's `__restrict`).
    unsafe {
        kernel(
            input.as_mut_ptr(),
            size(input.len()),
            window.as_mut_ptr(),
            out.as_mut_ptr(),
        );
    }
    out
}

/// `fft_window_ci16_cf32`: windows 16-bit `input` by `window` into a new float buffer.
///
/// # Panics
///
/// If the lengths differ or exceed `unsigned`.
#[must_use]
pub fn window_ci16(input: &[[i16; 2]], window: &[f32]) -> Vec<[f32; 2]> {
    assert_eq!(input.len(), window.len(), "one coefficient per sample");
    let mut input = input.to_vec();
    let mut window = window.to_vec();
    let mut out = vec![[f32::NAN; 2]; input.len()];
    // SAFETY: the selector only picks a function pointer.
    let kernel = unsafe { fft_window_ci16_cf32_c(OPT_GENERIC, ptr::null_mut()) };
    // SAFETY: as in `window_cf32`.
    unsafe {
        kernel(
            input.as_mut_ptr(),
            size(input.len()),
            window.as_mut_ptr(),
            out.as_mut_ptr(),
        );
    }
    out
}

/// A libusdr `fft_acc_t` with its own storage, driven by the float (`fftad_*`) or the
/// hardware (`fftad_*_hwi16`) kernels.
pub struct Accumulator {
    /// `f_mant`.
    mantissas: Vec<f32>,
    /// `f_pwr`.
    exponents: Vec<i32>,
    /// `mine`.
    floor: f32,
    /// Whether this runs the `_hwi16` kernels.
    hardware: bool,
}

impl Accumulator {
    /// A `bins`-point accumulator for complex spectra with power `floor`, initialized by
    /// `fftad_init`. Exponents start at a sentinel the C must overwrite.
    #[must_use]
    pub fn new(bins: usize, floor: f32) -> Self {
        let mut acc = Self::unset(bins, floor, false);
        acc.init();
        acc
    }

    /// A `bins`-point accumulator for hardware spectra, initialized by
    /// `fftad_init_hwi16`.
    #[must_use]
    pub fn new_hw(bins: usize) -> Self {
        let mut acc = Self::unset(bins, 0.0, true);
        acc.init();
        acc
    }

    /// Storage filled with sentinels, before `init`.
    fn unset(bins: usize, floor: f32, hardware: bool) -> Self {
        Self {
            mantissas: vec![f32::NAN; bins],
            exponents: vec![i32::MIN; bins],
            floor,
            hardware,
        }
    }

    /// The C struct over this storage, valid while `self` is not otherwise touched.
    fn raw(&mut self) -> FftAcc {
        FftAcc {
            f_mant: self.mantissas.as_mut_ptr(),
            f_pwr: self.exponents.as_mut_ptr(),
            mine: self.floor,
        }
    }

    /// `fftad_init` or `fftad_init_hwi16`.
    pub fn init(&mut self) {
        let bins = size(self.mantissas.len());
        // SAFETY: the selectors only pick a function pointer.
        let kernel = unsafe {
            if self.hardware {
                fftad_init_hwi16_c(OPT_GENERIC, ptr::null_mut())
            } else {
                fftad_init_c(OPT_GENERIC, ptr::null_mut())
            }
        };
        let mut raw = self.raw();
        // SAFETY: both arrays hold `bins` elements.
        unsafe { kernel(&raw mut raw, bins) };
    }

    /// `fftad_add` over a complex spectrum.
    ///
    /// # Panics
    ///
    /// If this is a hardware accumulator or the spectrum's size differs.
    pub fn add(&mut self, spectrum: &[[f32; 2]]) {
        assert!(!self.hardware, "a complex-spectrum accumulator");
        assert_eq!(spectrum.len(), self.mantissas.len(), "one value per bin");
        let mut spectrum = spectrum.to_vec();
        // SAFETY: the selector only picks a function pointer.
        let kernel = unsafe { fftad_add_c(OPT_GENERIC, ptr::null_mut()) };
        let mut raw = self.raw();
        // SAFETY: all three arrays hold `bins` elements and do not overlap.
        unsafe { kernel(&raw mut raw, spectrum.as_mut_ptr(), size(spectrum.len())) };
    }

    /// `fftad_add_hwi16` over a hardware spectrum.
    ///
    /// # Panics
    ///
    /// If this is not a hardware accumulator or the spectrum's size differs.
    pub fn add_hw(&mut self, spectrum: &[u16]) {
        assert!(self.hardware, "a hardware accumulator");
        assert_eq!(spectrum.len(), self.mantissas.len(), "one value per bin");
        let mut spectrum = spectrum.to_vec();
        // SAFETY: the selector only picks a function pointer.
        let kernel = unsafe { fftad_add_hwi16_c(OPT_GENERIC, ptr::null_mut()) };
        let mut raw = self.raw();
        // SAFETY: both arrays hold `bins` elements and do not overlap.
        unsafe { kernel(&raw mut raw, spectrum.as_mut_ptr(), size(spectrum.len())) };
    }

    /// `fftad_norm` or `fftad_norm_hwi16` over `out`, whose first `bins` elements are
    /// returned. `fftad_norm` writes bin `i` to `i ^ (bins / 2)`, past `bins` for sizes
    /// that are not powers of two; `out` gets room for that, and the overflow is dropped.
    ///
    /// # Panics
    ///
    /// If `out` does not hold one value per bin.
    #[must_use]
    pub fn normalize(&mut self, scale: f32, corr: f32, out: &[f32]) -> Vec<f32> {
        let bins = self.mantissas.len();
        assert_eq!(out.len(), bins, "one output per bin");
        let mut padded = out.to_vec();
        padded.resize(2 * bins.next_power_of_two(), f32::NAN);
        // SAFETY: the selectors only pick a function pointer.
        let kernel = unsafe {
            if self.hardware {
                fftad_norm_hwi16_c(OPT_GENERIC, ptr::null_mut())
            } else {
                fftad_norm_c(OPT_GENERIC, ptr::null_mut())
            }
        };
        let mut raw = self.raw();
        // SAFETY: both arrays hold `bins` elements; `padded` holds every `i ^ (bins / 2)`
        // for `i < bins`, all below twice the next power of two.
        unsafe { kernel(&raw mut raw, size(bins), scale, corr, padded.as_mut_ptr()) };
        padded.truncate(bins);
        padded
    }
}
