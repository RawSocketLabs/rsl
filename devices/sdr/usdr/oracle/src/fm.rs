//! libusdr's FM modulator and quadrature demodulator (`xdsp/fmquad.c`). IQ samples are
//! `[i, q]` pairs, interleaved as libusdr stores them.

use std::ffi::{c_int, c_uint};

/// `quadfm_decode_state_t`.
#[repr(C)]
struct DecodeState {
    /// `iq_prev`.
    iq_prev: [i16; 2],
    /// `d_mp`.
    d_mp: f32,
}

unsafe extern "C" {
    fn quadfm_encode(
        samples: c_uint,
        audio: *const i16,
        iq: *mut i16,
        gain: f32,
        iangle: f32,
    ) -> f32;
    fn quadfm_decode(
        state: *mut DecodeState,
        piq: *const i16,
        samples: c_uint,
        out: *mut i16,
        omaxp: *mut i32,
        opwr: *mut i64,
    ) -> c_int;
}

/// `quadfm_encode`: the IQ for `audio` and the phase after it.
///
/// # Panics
///
/// If `audio` is longer than `unsigned`.
#[must_use]
pub fn modulate(audio: &[i16], gain: f32, phase: f32) -> (Vec<[i16; 2]>, f32) {
    let mut iq = vec![[i16::MIN; 2]; audio.len()];
    let samples = c_uint::try_from(audio.len()).expect("fits unsigned");
    // SAFETY: `audio` holds `samples` values and `iq` `2 * samples`.
    let end =
        unsafe { quadfm_encode(samples, audio.as_ptr(), iq.as_mut_ptr().cast(), gain, phase) };
    (iq, end)
}

/// A libusdr demodulator state.
pub struct Demodulator(DecodeState);

impl Demodulator {
    /// A zeroed previous sample and output gain `gain`.
    #[must_use]
    pub fn new(gain: f32) -> Self {
        Self(DecodeState {
            iq_prev: [0; 2],
            d_mp: gain,
        })
    }

    /// `quadfm_decode`: the demodulated block, its peak power and its total power.
    ///
    /// # Panics
    ///
    /// If `iq` is longer than `unsigned`.
    #[must_use]
    pub fn demodulate(&mut self, iq: &[[i16; 2]]) -> (Vec<i16>, i32, i64) {
        let mut out = vec![i16::MIN; iq.len()];
        let (mut peak, mut total) = (i32::MIN, i64::MIN);
        let samples = c_uint::try_from(iq.len()).expect("fits unsigned");
        // SAFETY: `iq` holds `2 * samples` values and `out` `samples`; the state and the
        // two results are valid for writes.
        unsafe {
            quadfm_decode(
                &raw mut self.0,
                iq.as_ptr().cast(),
                samples,
                out.as_mut_ptr(),
                &raw mut peak,
                &raw mut total,
            );
        }
        (out, peak, total)
    }
}
