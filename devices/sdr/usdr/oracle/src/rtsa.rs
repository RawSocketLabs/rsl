//! libusdr's RTSA histogram (`xdsp/rtsa_functions.c`), called through its own dispatchers,
//! which return the generic (scalar) kernels in the vendored build.
//!
//! `rtsa_calc_depth` and `rtsa_fill_hwi16_consts` are `static inline` in the header, not
//! compiled symbols; calling them needs a C shim this crate does not have yet, so callers
//! supply the settings `rtsa_calc_depth` would complete.

use std::ffi::{c_char, c_uint};
use std::ptr;

/// `generic_opts_t::OPT_GENERIC`.
const OPT_GENERIC: c_uint = 0;

/// `fft_rtsa_settings_t`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// `upper_pwr_bound`, dB.
    pub upper_pwr_bound: i16,
    /// `lower_pwr_bound`, dB.
    pub lower_pwr_bound: i16,
    /// `divs_for_dB`.
    pub divs_for_db: c_uint,
    /// `rtsa_depth`.
    pub rtsa_depth: c_uint,
    /// `charging_frame`.
    pub charging_frame: c_uint,
    /// `raise_coef`.
    pub raise_coef: c_uint,
    /// `decay_coef`.
    pub decay_coef: c_uint,
}

/// `fft_rtsa_data_t`.
#[repr(C)]
struct RtsaData {
    /// The settings.
    settings: Settings,
    /// `rtsa_depth * fft_size` cells.
    pwr: *mut u16,
}

/// `fft_diap_t`.
#[repr(C)]
#[derive(Clone, Copy)]
struct Diap {
    /// First bin.
    from: c_uint,
    /// One past the last bin.
    to: c_uint,
}

/// `rtsa_hwi16_consts_t`, which the generic kernel ignores.
#[repr(C)]
#[derive(Default)]
struct HwConsts {
    /// `org_scale`.
    org_scale: f32,
    /// `nfft`, `c0`, `c1`, `ndivs_for_dB`, `shr0`, `shr1`.
    fields: [u16; 6],
}

/// `rtsa_update_function_t`.
type UpdateFn = unsafe extern "C" fn(*mut [f32; 2], c_uint, *mut RtsaData, f32, f32, f32, Diap);
/// `rtsa_update_hwi16_function_t`.
type UpdateHwFn =
    unsafe extern "C" fn(*mut u16, c_uint, *mut RtsaData, f32, f32, Diap, *const HwConsts);

unsafe extern "C" {
    fn rsl_oracle_rtsa_calc_depth(st: *mut Settings);
    fn rtsa_init(rtsa_data: *mut RtsaData, fft_size: c_uint);
    fn rtsa_update_c(cpu_cap: c_uint, sfunc: *mut *const c_char) -> UpdateFn;
    fn rtsa_update_hwi16_c(cpu_cap: c_uint, sfunc: *mut *const c_char) -> UpdateHwFn;
}

/// An RTSA histogram libusdr owns the arithmetic of.
pub struct Rtsa {
    /// The settings, depth included.
    settings: Settings,
    /// FFT bins.
    bins: usize,
    /// The cells.
    cells: Vec<u16>,
}

/// `value` as libusdr's `unsigned`.
fn unsigned(value: usize) -> c_uint {
    c_uint::try_from(value).expect("fits unsigned")
}

impl Rtsa {
    /// A histogram cleared by `rtsa_init` (from a non-zero fill, so the clearing shows).
    ///
    /// # Panics
    ///
    /// If the cells cannot be addressed, `bins` exceeds `unsigned`, or the settings are
    /// ones the C kernels would index out of bounds with or divide by zero with: a depth
    /// outside `1..=2^24`, or a zero step count, charging frame or decay.
    #[must_use]
    pub fn new(settings: Settings, bins: usize) -> Self {
        assert!(
            (1..=1 << 24).contains(&settings.rtsa_depth),
            "a depth whose clamped index stays in the column"
        );
        assert!(
            settings.divs_for_db != 0 && settings.charging_frame != 0 && settings.decay_coef != 0,
            "no zero divisor or decay period"
        );
        let depth = usize::try_from(settings.rtsa_depth).expect("depth fits usize");
        let mut rtsa = Self {
            settings,
            bins,
            cells: vec![0xa5a5; depth * bins],
        };
        let mut data = rtsa.raw();
        // SAFETY: `pwr` holds `rtsa_depth * fft_size` cells, the count `rtsa_init` clears.
        unsafe { rtsa_init(&raw mut data, unsigned(bins)) };
        rtsa
    }

    /// The C struct over this storage.
    fn raw(&mut self) -> RtsaData {
        RtsaData {
            settings: self.settings,
            pwr: self.cells.as_mut_ptr(),
        }
    }

    /// The cells, bin-major.
    #[must_use]
    pub fn cells(&self) -> &[u16] {
        &self.cells
    }

    /// The bins `rtsa_update` covers for a `len`-bin spectrum from `first`.
    fn diap(&self, first: usize, len: usize) -> Diap {
        assert!(first + len <= self.bins, "the spectrum fits the histogram");
        let settings = self.settings;
        let period = u64::from(settings.charging_frame) * u64::from(settings.decay_coef);
        assert!(period < 1 << 31, "a decay shift below 32");
        Diap {
            from: unsigned(first),
            to: unsigned(first + len),
        }
    }

    /// `rtsa_update` with `diap = {first, first + spectrum.len()}`.
    ///
    /// # Panics
    ///
    /// If the spectrum reaches past the histogram, a level parameter is not finite, or
    /// `charging_frame * decay_coef` reaches 2^31 (a decay shift of 32).
    pub fn update(
        &mut self,
        first: usize,
        spectrum: &[[f32; 2]],
        scale: f32,
        floor: f32,
        corr: f32,
    ) {
        assert!(
            scale.is_finite() && floor.is_finite() && corr.is_finite(),
            "a finite level, so its index conversion is defined"
        );
        let diap = self.diap(first, spectrum.len());
        let mut spectrum = spectrum.to_vec();
        // SAFETY: the selector only picks a function pointer.
        let kernel = unsafe { rtsa_update_c(OPT_GENERIC, ptr::null_mut()) };
        let bins = unsigned(self.bins);
        let mut data = self.raw();
        // SAFETY: `spectrum` holds `to - from` bins and `pwr` covers every bin below
        // `fft_size >= to`. `new` bounded the depth to 1..=2^24, where the clamped float
        // index stays below it, and the levels are finite, so each charged index converts
        // and lands in its column.
        unsafe {
            kernel(
                spectrum.as_mut_ptr(),
                bins,
                &raw mut data,
                scale,
                floor,
                corr,
                diap,
            );
        }
    }

    /// `rtsa_update_hwi16` with `diap = {first, first + spectrum.len()}`.
    ///
    /// # Panics
    ///
    /// If the spectrum reaches past the histogram, a level parameter is not finite, or
    /// `charging_frame * decay_coef` reaches 2^31 (a decay shift of 32).
    pub fn update_hw(&mut self, first: usize, spectrum: &[u16], scale: f32, corr: f32) {
        assert!(
            scale.is_finite() && corr.is_finite(),
            "a finite level, so its index conversion is defined"
        );
        let diap = self.diap(first, spectrum.len());
        let mut spectrum = spectrum.to_vec();
        let consts = HwConsts::default();
        // SAFETY: the selector only picks a function pointer.
        let kernel = unsafe { rtsa_update_hwi16_c(OPT_GENERIC, ptr::null_mut()) };
        let bins = unsigned(self.bins);
        let mut data = self.raw();
        // SAFETY: as in `update`; `consts` is valid (and unread by the generic kernel).
        unsafe {
            kernel(
                spectrum.as_mut_ptr(),
                bins,
                &raw mut data,
                scale,
                corr,
                diap,
                &raw const consts,
            );
        }
    }
}

/// libusdr's `rtsa_calc_depth` under the generic build: fills `rtsa_depth` and lowers
/// `lower_pwr_bound` to the aligned depth.
///
/// # Panics
///
/// Unless `upper_pwr_bound > lower_pwr_bound`, which the C asserts.
#[must_use]
pub fn calc_depth(mut settings: Settings) -> Settings {
    assert!(
        settings.upper_pwr_bound > settings.lower_pwr_bound,
        "the C asserts upper > lower"
    );
    // SAFETY: `settings` is a valid, exclusively borrowed `fft_rtsa_settings_t`.
    unsafe { rsl_oracle_rtsa_calc_depth(&raw mut settings) };
    settings
}
