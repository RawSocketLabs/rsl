//! libusdr's calibration mathematics (`cal/opt_func.c`, `cal/cal_filt.c`), called directly:
//! these functions are pure, so no board or session is involved.

use std::ffi::{c_int, c_uint, c_void};

/// `cfolat64_t`: one IQ sample as two floats.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CFloat {
    /// In-phase.
    pub i: f32,
    /// Quadrature.
    pub q: f32,
}

/// `evaluate_fn_t`.
type EvaluateFn = unsafe extern "C" fn(*mut c_void, c_int, *mut c_int) -> c_int;
/// `evaluate_fnN_t`.
type EvaluateFnN = unsafe extern "C" fn(*mut c_void, c_int, c_int, *mut c_int) -> c_int;
/// `search_function_t`.
type SearchFn = unsafe extern "C" fn(
    c_int,
    c_int,
    *mut c_void,
    EvaluateFn,
    *mut c_int,
    *mut c_int,
    c_int,
) -> c_int;

/// `opt_par_limits_t`.
#[repr(C)]
#[derive(Clone, Copy)]
struct Limits {
    min: c_int,
    max: c_int,
}

/// `struct opt_iteration2d`.
#[repr(C)]
#[derive(Clone, Copy)]
struct Iteration2d {
    func: EvaluateFnN,
    sf: SearchFn,
    limit: [Limits; 2],
    exparam: c_int,
}

unsafe extern "C" {
    fn convert_dump12_to_cfloat(input: *const u32, out: *mut CFloat, size: c_uint);
    fn calculate_mean_s_std(d: *const CFloat, size: c_uint, mean: *mut f32, std: *mut f32);
    fn cal_generate_iq(d: *mut CFloat, size: c_uint, amp: f32, period: f32, ph: f32);
    fn cal_remove_dc(d: *mut CFloat, size: c_uint, avg_i: *mut f32, avg_q: *mut f32);
    fn cal_find_best_fit(
        d: *const CFloat,
        size: c_uint,
        amp: f32,
        period: f32,
        mse: *mut f32,
        phase: *mut f32,
    );
    fn cal_find_best_fit_ph_per(
        d: *const CFloat,
        size: c_uint,
        amp: f32,
        period: *mut f32,
        mse: *mut f32,
        phase: *mut f32,
    );
    fn cal_max_amp_error(d: *const CFloat, size: c_uint, amp: f32, period: f32, ph: f32) -> f32;
    fn _cal_mse_fit(d: *const CFloat, size: c_uint, amp: f32, period: f32, ph: f32) -> f32;
    fn find_golden_min(
        start: c_int,
        stop: c_int,
        param: *mut c_void,
        f: EvaluateFn,
        px: *mut c_int,
        pfx: *mut c_int,
        exparam: c_int,
    ) -> c_int;
    fn find_iterate_min(
        start: c_int,
        stop: c_int,
        param: *mut c_void,
        f: EvaluateFn,
        px: *mut c_int,
        pfx: *mut c_int,
        exparam: c_int,
    ) -> c_int;
    fn find_best_2d(
        ops: *mut Iteration2d,
        max_count: c_uint,
        param: *mut c_void,
        stop_when: c_int,
        px: *mut c_int,
        py: *mut c_int,
        pfxy: *mut c_int,
    ) -> c_int;
    fn find_gcd(a: u64, b: u64) -> u64;
}

/// A slice length as C's `unsigned`.
fn size(len: usize) -> c_uint {
    c_uint::try_from(len).expect("test vectors fit in unsigned")
}

/// `convert_dump12_to_cfloat`.
#[must_use]
pub fn dump12(words: &[u32]) -> Vec<CFloat> {
    let mut out = vec![CFloat::default(); words.len()];
    // SAFETY: both buffers hold `len` elements; the C writes only `out`.
    unsafe { convert_dump12_to_cfloat(words.as_ptr(), out.as_mut_ptr(), size(words.len())) };
    out
}

/// `calculate_mean_s_std`.
#[must_use]
pub fn mean_std(d: &[CFloat]) -> (f32, f32) {
    let (mut mean, mut std) = (0.0, 0.0);
    // SAFETY: `d` holds `len` samples; the outputs are valid floats.
    unsafe { calculate_mean_s_std(d.as_ptr(), size(d.len()), &raw mut mean, &raw mut std) };
    (mean, std)
}

/// `cal_generate_iq`.
#[must_use]
pub fn generate(len: usize, amp: f32, period: f32, ph: f32) -> Vec<CFloat> {
    let mut out = vec![CFloat::default(); len];
    // SAFETY: `out` holds `len` samples.
    unsafe { cal_generate_iq(out.as_mut_ptr(), size(len), amp, period, ph) };
    out
}

/// `cal_remove_dc`: centres `d` and returns the removed mean.
pub fn remove_dc(d: &mut [CFloat]) -> CFloat {
    let mut mean = CFloat::default();
    // SAFETY: `d` holds `len` samples; the outputs are valid floats.
    unsafe {
        cal_remove_dc(
            d.as_mut_ptr(),
            size(d.len()),
            &raw mut mean.i,
            &raw mut mean.q,
        );
    }
    mean
}

/// `cal_find_best_fit`: (error, phase).
#[must_use]
pub fn best_fit(d: &[CFloat], amp: f32, period: f32) -> (f32, f32) {
    let (mut mse, mut phase) = (0.0, 0.0);
    // SAFETY: `d` holds `len` samples; the outputs are valid floats.
    unsafe {
        cal_find_best_fit(
            d.as_ptr(),
            size(d.len()),
            amp,
            period,
            &raw mut mse,
            &raw mut phase,
        );
    };
    (mse, phase)
}

/// `cal_find_best_fit_ph_per`: (error, phase, period).
#[must_use]
pub fn best_fit_phase_period(d: &[CFloat], amp: f32) -> (f32, f32, f32) {
    let (mut mse, mut phase, mut period) = (0.0, 0.0, 0.0);
    // SAFETY: `d` holds `len` samples; the outputs are valid floats.
    unsafe {
        cal_find_best_fit_ph_per(
            d.as_ptr(),
            size(d.len()),
            amp,
            &raw mut period,
            &raw mut mse,
            &raw mut phase,
        );
    };
    (mse, phase, period)
}

/// `cal_max_amp_error`.
#[must_use]
pub fn max_amp_error(d: &[CFloat], amp: f32, period: f32, ph: f32) -> f32 {
    // SAFETY: `d` holds `len` samples.
    unsafe { cal_max_amp_error(d.as_ptr(), size(d.len()), amp, period, ph) }
}

/// `_cal_mse_fit`: the summed distance between `d` and a tone.
#[must_use]
pub fn fit_error(d: &[CFloat], amp: f32, period: f32, ph: f32) -> f32 {
    // SAFETY: `d` holds `len` samples.
    unsafe { _cal_mse_fit(d.as_ptr(), size(d.len()), amp, period, ph) }
}

/// `find_gcd`.
#[must_use]
pub fn gcd(a: u64, b: u64) -> u64 {
    // SAFETY: pure arithmetic.
    unsafe { find_gcd(a, b) }
}

/// A cost function the C calls back; its calls are recorded by the caller's closure.
type Cost<'a> = &'a mut dyn FnMut(i32) -> i32;

/// `evaluate_fn_t` over a [`Cost`].
unsafe extern "C" fn evaluate(param: *mut c_void, value: c_int, func: *mut c_int) -> c_int {
    // SAFETY: `param` is the `Cost` the wrapper passed, alive for the C call.
    let cost = unsafe { &mut *param.cast::<Cost>() };
    // SAFETY: the C passes a valid result pointer.
    unsafe { *func = cost(value) };
    0
}

/// Runs a C one-dimensional search: `(x, cost)`.
fn search_1d(
    search: SearchFn,
    start: i32,
    stop: i32,
    exparam: i32,
    mut cost: impl FnMut(i32) -> i32,
) -> (i32, i32) {
    let mut cost: Cost = &mut cost;
    let (mut x, mut fx) = (0, 0);
    // SAFETY: `param` points at `cost` for the duration; outputs are valid ints.
    let status = unsafe {
        search(
            start,
            stop,
            (&raw mut cost).cast(),
            evaluate,
            &raw mut x,
            &raw mut fx,
            exparam,
        )
    };
    assert_eq!(status, 0, "the evaluator never fails");
    (x, fx)
}

/// `find_golden_min`: `(x, cost)`.
///
/// # Panics
///
/// If the C reports an evaluator failure, which these evaluators never cause.
pub fn golden_min(start: i32, stop: i32, cost: impl FnMut(i32) -> i32) -> (i32, i32) {
    search_1d(find_golden_min, start, stop, 0, cost)
}

/// `find_iterate_min`: `(x, cost)`.
///
/// # Panics
///
/// As [`golden_min`].
pub fn iterate_min(start: i32, stop: i32, extra: i32, cost: impl FnMut(i32) -> i32) -> (i32, i32) {
    search_1d(find_iterate_min, start, stop, extra, cost)
}

/// A two-dimensional objective the C calls back: `(axis, value, evaluate) -> cost`.
type Cost2d<'a> = &'a mut dyn FnMut(i32, i32, bool) -> i32;

/// `evaluate_fnN_t` over a [`Cost2d`].
unsafe extern "C" fn evaluate_2d(
    param: *mut c_void,
    axis: c_int,
    value: c_int,
    func: *mut c_int,
) -> c_int {
    // SAFETY: `param` is the `Cost2d` the wrapper passed, alive for the C call.
    let cost = unsafe { &mut *param.cast::<Cost2d>() };
    let result = cost(axis, value, !func.is_null());
    if !func.is_null() {
        // SAFETY: non-null result pointers from the C are valid.
        unsafe { *func = result };
    }
    0
}

/// One `find_best_2d` stage: golden (true) or iterate, X and Y limits, extra step.
#[derive(Clone, Copy, Debug)]
pub struct Stage {
    /// Use `find_golden_min`, else `find_iterate_min`.
    pub golden: bool,
    /// X then Y limits, `(min, max)`.
    pub limits: [(i32, i32); 2],
    /// `exparam`.
    pub extra: i32,
}

/// `find_best_2d`: `(x, y, cost)`.
///
/// # Panics
///
/// As [`golden_min`].
pub fn best_2d(
    stages: &[Stage],
    stop_when: i32,
    mut cost: impl FnMut(i32, i32, bool) -> i32,
) -> (i32, i32, i32) {
    let mut ops: Vec<Iteration2d> = stages
        .iter()
        .map(|stage| Iteration2d {
            func: evaluate_2d,
            sf: if stage.golden {
                find_golden_min
            } else {
                find_iterate_min
            },
            limit: stage.limits.map(|(min, max)| Limits { min, max }),
            exparam: stage.extra,
        })
        .collect();
    let mut cost: Cost2d = &mut cost;
    let (mut x, mut y, mut fxy) = (0, 0, 0);
    // SAFETY: `ops` holds `len` stages; `param` points at `cost` for the duration.
    let status = unsafe {
        find_best_2d(
            ops.as_mut_ptr(),
            size(ops.len()),
            (&raw mut cost).cast(),
            stop_when,
            &raw mut x,
            &raw mut y,
            &raw mut fxy,
        )
    };
    assert_eq!(status, 0, "the evaluator never fails");
    (x, y, fxy)
}
