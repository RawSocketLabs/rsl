//! libusdr's host sample conversions (`xdsp/conv.c`), selected by its own dispatcher.
//!
//! The vendored build defines no `WVLT_ARCH_*`, so `vbase.h` compiles no SIMD variant and
//! `get_transform_fn` always returns the generic (scalar) kernels: the reference
//! `usdr::dsp` ports.

use std::ffi::{CString, c_char, c_uint, c_void};

/// `conv_function_t`.
type ConvFn = unsafe extern "C" fn(*const *const c_void, c_uint, *const *mut c_void, c_uint);
/// `size_function_t`.
type SizeFn = unsafe extern "C" fn(c_uint, bool) -> c_uint;

/// `transform_info_t`: the kernel and its size rule; both null for no converter.
#[repr(C)]
struct TransformInfo {
    /// The kernel.
    cfunc: Option<ConvFn>,
    /// Its size rule.
    sfunc: Option<SizeFn>,
}

unsafe extern "C" {
    fn get_transform_fn(
        from: *const c_char,
        to: *const c_char,
        inveccnt: c_uint,
        outveccnt: c_uint,
    ) -> TransformInfo;
    fn is_transform_dummy(t: Option<ConvFn>) -> bool;
}

/// Bytes past each buffer's end that libusdr's kernels may touch: some read 8 bytes where
/// 6 remain, and the `ci12` to two `cf32` tail can write 4 bytes past a short channel.
const SLACK: usize = 16;
/// Fill for the slack, to detect writes into it.
const GUARD: u8 = 0x5a;

/// A converter libusdr selected.
pub struct Transform {
    /// The kernel.
    cfunc: ConvFn,
    /// Its size rule.
    sfunc: SizeFn,
    /// Input channel count.
    inputs: usize,
    /// Output channel count.
    outputs: usize,
}

/// libusdr's `get_transform_fn(from, to, inputs, outputs)`; `None` when it returns no
/// converter.
///
/// # Panics
///
/// If a name holds a NUL byte or a count exceeds `unsigned`.
#[must_use]
pub fn transform(from: &str, to: &str, inputs: usize, outputs: usize) -> Option<Transform> {
    let from = CString::new(from).expect("format names hold no NUL");
    let to = CString::new(to).expect("format names hold no NUL");
    let counts = (
        c_uint::try_from(inputs).expect("channel count fits unsigned"),
        c_uint::try_from(outputs).expect("channel count fits unsigned"),
    );
    // SAFETY: both names are NUL-terminated and outlive the call, which only compares them.
    let info = unsafe { get_transform_fn(from.as_ptr(), to.as_ptr(), counts.0, counts.1) };
    let (Some(cfunc), Some(sfunc)) = (info.cfunc, info.sfunc) else {
        return None;
    };
    Some(Transform {
        cfunc,
        sfunc,
        inputs,
        outputs,
    })
}

/// What a C kernel left in the output buffers.
pub struct Run {
    /// Each output buffer after the call, at its declared length.
    pub outputs: Vec<Vec<u8>>,
    /// Whether the kernel wrote past any output buffer's declared length.
    pub overran: bool,
}

impl Transform {
    /// Whether libusdr chose its `tr_dummy` byte copy.
    #[must_use]
    pub fn is_dummy(&self) -> bool {
        // SAFETY: a pure pointer comparison.
        unsafe { is_transform_dummy(Some(self.cfunc)) }
    }

    /// The size rule: output bytes for `bytes` input bytes, or input bytes for `bytes`
    /// output bytes when `reverse`.
    #[must_use]
    pub fn size(&self, bytes: u32, reverse: bool) -> u32 {
        // SAFETY: the size functions are pure arithmetic.
        unsafe { (self.sfunc)(bytes, reverse) }
    }

    /// Runs the kernel on `inputs`, one buffer per channel, over `outputs`' initial
    /// contents, passing their total lengths as libusdr's callers do.
    ///
    /// # Panics
    ///
    /// If the buffer counts differ from the transform's, one side's buffers differ in
    /// length, or a total exceeds `unsigned`.
    #[must_use]
    pub fn run(&self, inputs: &[&[u8]], outputs: &[Vec<u8>]) -> Run {
        assert_eq!(inputs.len(), self.inputs, "one buffer per input channel");
        assert_eq!(outputs.len(), self.outputs, "one buffer per output channel");
        let input_len = inputs.first().map_or(0, |input| input.len());
        let output_len = outputs.first().map_or(0, Vec::len);
        assert!(inputs.iter().all(|input| input.len() == input_len));
        assert!(outputs.iter().all(|output| output.len() == output_len));
        let input_total = c_uint::try_from(input_len * inputs.len()).expect("fits unsigned");
        let output_total = c_uint::try_from(output_len * outputs.len()).expect("fits unsigned");

        let input_words: Vec<Vec<u64>> = inputs.iter().map(|input| padded(input)).collect();
        let mut output_words: Vec<Vec<u64>> = outputs.iter().map(|output| padded(output)).collect();
        let input_ptrs: Vec<*const c_void> = input_words
            .iter()
            .map(|words| words.as_ptr().cast())
            .collect();
        let output_ptrs: Vec<*mut c_void> = output_words
            .iter_mut()
            .map(|words| words.as_mut_ptr().cast())
            .collect();
        // SAFETY: there is one 8-byte-aligned pointer per channel the kernel was selected
        // for, each valid for its declared length plus `SLACK` bytes (the furthest any
        // generic kernel reads or writes past a declared length), and the totals passed are
        // the declared lengths summed. The buffers outlive the call and do not overlap.
        unsafe {
            (self.cfunc)(
                input_ptrs.as_ptr(),
                input_total,
                output_ptrs.as_ptr(),
                output_total,
            );
        };
        let mut overran = false;
        let outputs = output_words
            .iter()
            .map(|words| {
                let mut bytes: Vec<u8> = words.iter().copied().flat_map(u64::to_ne_bytes).collect();
                overran |= bytes[output_len..output_len + SLACK]
                    .iter()
                    .any(|&byte| byte != GUARD);
                bytes.truncate(output_len);
                bytes
            })
            .collect();
        Run { outputs, overran }
    }
}

/// `bytes` in 8-byte-aligned storage followed by at least `SLACK` guard bytes.
fn padded(bytes: &[u8]) -> Vec<u64> {
    let mut storage = bytes.to_vec();
    storage.resize((bytes.len() + SLACK).next_multiple_of(8), GUARD);
    storage
        .chunks_exact(8)
        .map(|word| u64::from_ne_bytes(word.try_into().expect("8-byte chunks")))
        .collect()
}
