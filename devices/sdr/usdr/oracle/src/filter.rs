//! libusdr's block FIR filter: `xdsp/filter.c` (`filter_data_*`) over the kernels
//! `xdsp/conv_filter.c` selects.
//!
//! The vendored build defines no `WVLT_ARCH_*`, so `vbase.h` compiles no SIMD kernel and
//! `cpu_vcap_get` stays `OPT_GENERIC`: `filter_data_alloc` selects the generic kernels and
//! never applies its AVX2 tap reordering. Those kernels are what `usdr::dsp::filter`
//! ports.

use std::ffi::{c_uint, c_void};
use std::ptr::NonNull;
use std::slice;

/// `FDAF_INTERLEAVE`: complex values, I and Q interleaved.
pub const INTERLEAVE: u32 = 1;
/// `FDAF_SEPARATED`: complex values, all I then all Q (libusdr rejects it).
pub const SEPARATED: u32 = 2;
/// `FDAF_INTERPOLATE`: interpolate by `decim_inter` instead of decimating.
pub const INTERPOLATE: u32 = 16;

/// Values past the output's expected end in which writes are detected.
const SLACK: usize = 64;
/// Fill for the output and its slack, to detect what the C writes.
const GUARD: i16 = 0x5a5a;

unsafe extern "C" {
    fn filter_data_alloc(
        origblksz: c_uint,
        pfilter: *const i16,
        filer_taps: c_uint,
        decim_inter: c_uint,
        flags: c_uint,
    ) -> *mut c_void;
    fn filter_data_free(o: *mut c_void);
    fn filter_data_ptr(o: *mut c_void) -> *mut i16;
    fn filter_data_process(o: *mut c_void, out: *mut i16);
    fn filter_block_size(o: *mut c_void) -> c_uint;
}

/// A `filter_data_t`, freed on drop.
pub struct Filter {
    /// The C object.
    raw: NonNull<c_void>,
    /// Its block length, in values.
    block: usize,
    /// `decim_inter`: the interpolation factor, or the decimation exponent.
    decim_inter: usize,
    /// The allocation flags.
    flags: u32,
}

/// What one `filter_data_process` call wrote.
pub struct Run {
    /// The values the C kernel's loop stores to ([`Filter::writes`]), [`GUARD`]-filled
    /// where it did not.
    pub outputs: Vec<i16>,
    /// Whether the C wrote past them.
    pub overran: bool,
}

/// Whether libusdr's `assert(tapssz >= ftaps * decim_inter * sizeof(int16_t))` holds for an
/// interpolating filter: its polyphase rows fit the taps rounded up to a 64-byte line.
#[must_use]
pub fn polyphase_fits(taps: usize, factor: u32) -> bool {
    let factor = factor as usize;
    factor != 0 && taps.div_ceil(factor) * factor <= taps.next_multiple_of(32)
}

impl Filter {
    /// `filter_data_alloc(block, taps, taps.len(), decim_inter, flags)`; `None` when it
    /// returns NULL.
    ///
    /// When interpolating, libusdr's polyphase split reads up to `decim_inter` taps past the
    /// end; the taps passed are followed by that many zeros, so those reads are defined.
    ///
    /// # Panics
    ///
    /// If a count exceeds `unsigned`, or the shape is one where the C is undefined: a
    /// decimation exponent over 30 (`1 << decim_bits` overflows), or interpolation by zero
    /// (a division) or with rows that fail [`polyphase_fits`] (written past the taps once
    /// `NDEBUG` drops libusdr's `assert`).
    #[must_use]
    pub fn new(taps: &[i16], block: usize, decim_inter: u32, flags: u32) -> Option<Self> {
        let interpolate = flags & INTERPOLATE != 0;
        if interpolate {
            assert!(
                polyphase_fits(taps.len(), decim_inter),
                "polyphase rows fit"
            );
        } else {
            assert!(decim_inter <= 30, "the decimation shift fits");
        }
        let mut padded = taps.to_vec();
        if interpolate {
            padded.resize(taps.len() + decim_inter as usize, 0);
        }
        let count = |n: usize| c_uint::try_from(n).expect("fits unsigned");
        // SAFETY: `padded` holds `taps.len()` taps and, when interpolating, the furthest
        // index the polyphase split reads (`decim_inter * ceil(taps / decim_inter) - 1`);
        // it is only read during the call, which copies the taps. The shape is one the
        // asserts above admit, for which the C's own tap and block buffers suffice.
        let raw = unsafe {
            filter_data_alloc(
                count(block),
                padded.as_ptr(),
                count(taps.len()),
                decim_inter,
                flags,
            )
        };
        Some(Self {
            raw: NonNull::new(raw)?,
            block,
            decim_inter: decim_inter as usize,
            flags,
        })
    }

    /// `filter_block_size`.
    #[must_use]
    pub fn block_size(&self) -> u32 {
        // SAFETY: `raw` is a live object from `filter_data_alloc`.
        unsafe { filter_block_size(self.raw.as_ptr()) }
    }

    /// How many values one `filter_data_process` call writes: one past the furthest index
    /// the selected generic kernel stores to, its loop transcribed from
    /// `templates/conv_filter*_generic.t` (`count` is the block, `d` the exponent or
    /// factor), independently of the Rust port.
    #[must_use]
    pub fn writes(&self) -> usize {
        let (count, d) = (self.block, self.decim_inter);
        let interleave = self.flags & INTERLEAVE != 0;
        if self.flags & INTERPOLATE == 0 {
            // out[(n >> decim_bits) + 0] (and + 1 interleaved), n += 1 << d (2 << d).
            let step = if interleave { 2 << d } else { 1 << d };
            (0..count)
                .step_by(step)
                .map(|n| (n >> d) + usize::from(interleave) + 1)
                .max()
        } else if interleave {
            // out[interp * n + 2 * z + 1] for z < interp, n += 2.
            (0..count).step_by(2).map(|n| d * n + 2 * d).max()
        } else {
            // out[interp * n + z] for z < interp.
            (0..count).map(|n| d * n + d).max()
        }
        .unwrap_or(0)
    }

    /// Fills the block at `filter_data_ptr` with `input`, then runs `filter_data_process`
    /// into a buffer of [`writes`](Self::writes) values plus slack.
    ///
    /// # Panics
    ///
    /// If `input` is not one block.
    #[must_use]
    pub fn process(&mut self, input: &[i16]) -> Run {
        assert_eq!(input.len(), self.block, "one block of input");
        // SAFETY: `filter_data_ptr` points at the object's block area of `block` values,
        // which nothing else references while `self` is borrowed mutably.
        let block =
            unsafe { slice::from_raw_parts_mut(filter_data_ptr(self.raw.as_ptr()), self.block) };
        block.copy_from_slice(input);
        let outputs = self.writes();
        let mut out = vec![GUARD; outputs + SLACK];
        // SAFETY: `raw` is live, and `out` holds `writes()` values, every index the generic
        // kernel's loop stores to for this block, plus guarded slack.
        unsafe { filter_data_process(self.raw.as_ptr(), out.as_mut_ptr()) };
        let overran = out[outputs..].iter().any(|&value| value != GUARD);
        out.truncate(outputs);
        Run {
            outputs: out,
            overran,
        }
    }
}

impl Drop for Filter {
    fn drop(&mut self) {
        // SAFETY: `raw` came from `filter_data_alloc` and is freed once, here.
        unsafe { filter_data_free(self.raw.as_ptr()) };
    }
}
