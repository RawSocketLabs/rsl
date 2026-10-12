//! FIR filtering of 16-bit samples, decimating or interpolating, block by block with history
//! carried between blocks: the scalar port of libusdr's `xdsp/filter.c` (`filter_data_*`)
//! and the generic kernels `xdsp/conv_filter.c` selects
//! (`templates/conv_filter*_generic.t`).
//!
//! Taps are Q15. Each output is a 32-bit sum of `sample * tap` products, shifted right
//! arithmetically by 15 (by `15 - log2(factor)` when interpolating by 1, 2, 4, 8 or 16, by
//! 10 for any other factor) and truncated to 16 bits, as in the C; the checks against the
//! compiled C are the oracle's `filter_parity` test.
//!
//! [`Fir`] keeps a window of the last `taps` values (twice that for [`Layout::Interleaved`])
//! followed by the current block. Each output's sum runs forward over the window from its
//! position, so the window's history makes the filter's delay line. Where libusdr is
//! undefined, this is defined instead:
//!
//! - The 32-bit sums overflow (signed, undefined in C) only for taps whose absolute sum
//!   reaches 65536; here they wrap.
//! - With a block shorter than the history, libusdr's history copy overlaps (`memcpy`,
//!   undefined); here it moves as `memmove` would.
//! - Interpolating with a tap count that is not a multiple of the factor, libusdr's
//!   polyphase split reads past the caller's taps; here the missing taps are zero.
//! - libusdr divides by an interpolation factor of zero, which [`Rate::Interpolate`]'s
//!   `NonZeroU32` cannot hold, and shifts by decimation exponents up to any `unsigned`,
//!   which [`Fir::new`] rejects beyond [`MAX_DECIMATION_LOG2`].
//! - libusdr's sizes are 32-bit and its interpolation `assert`s that the polyphase rows fit
//!   its tap buffer; here sizes are exact `usize`, any factor works, and [`Fir::new`]
//!   rejects shapes whose window or outputs would not fit `usize`.
//!
//! libusdr's other choices are kept: the same `decim_inter` argument is a factor when
//! interpolating but an exponent when decimating (2 decimates by four), here
//! [`Rate::Decimate`]'s exponent; the interpolating window keeps all `taps` values of
//! history although each output sums only `taps / factor` of them; and an interleaved block
//! of odd length shifts the history by an odd count, swapping I and Q in it. libusdr's
//! separated-complex layout (`FDAF_SEPARATED`) is unimplemented there (allocation fails),
//! and so absent here. Its SSSE3/AVX2 kernels and their tap reordering are not ported.

use core::error::Error;
use core::fmt;
use core::num::NonZeroU32;

/// Largest [`Rate::Decimate`] exponent.
pub const MAX_DECIMATION_LOG2: u32 = 30;

/// How samples lie in a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    /// Real values.
    Real,
    /// Complex values, I and Q interleaved (`FDAF_INTERLEAVE`); both are filtered with the
    /// same real taps.
    Interleaved,
}

/// The rate change a [`Fir`] applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rate {
    /// Keep one output in `2^log2`, `log2` at most [`MAX_DECIMATION_LOG2`]
    /// (`decim_inter = log2`). `Decimate(0)` filters at the input rate.
    Decimate(u32),
    /// `factor` outputs per input from a polyphase split of the taps (`FDAF_INTERPOLATE`,
    /// `decim_inter = factor`).
    Interpolate(NonZeroU32),
}

/// A block FIR filter with its history: libusdr's `filter_data_t`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fir {
    /// Sample arrangement.
    layout: Layout,
    /// Rate change.
    rate: Rate,
    /// Taps in kernel order: as given when decimating, polyphase rows of `len` when
    /// interpolating (`filter_data`).
    taps: Vec<i16>,
    /// Taps per output sum (`ftaps`).
    len: usize,
    /// History, then the current block (`history_data`).
    window: Vec<i16>,
    /// Values of history at the window's start.
    history: usize,
    /// Values written per block.
    outputs: usize,
}

impl Fir {
    /// A filter over blocks of `block_len` values (a complex sample is two) with `taps`, its
    /// history zeroed (`filter_data_alloc`).
    ///
    /// # Errors
    ///
    /// [`FirError::Decimation`] for a decimation exponent beyond [`MAX_DECIMATION_LOG2`],
    /// and [`FirError::TooLarge`] when the window, polyphase taps or outputs would not fit
    /// `usize`.
    pub fn new(
        taps: &[i16],
        layout: Layout,
        rate: Rate,
        block_len: usize,
    ) -> Result<Self, FirError> {
        match rate {
            Rate::Decimate(log2) if log2 > MAX_DECIMATION_LOG2 => {
                return Err(FirError::Decimation(log2));
            }
            Rate::Decimate(_) | Rate::Interpolate(_) => {}
        }
        let given = taps.len();
        let history = match layout {
            Layout::Real => Some(given),
            Layout::Interleaved => given.checked_mul(2),
        };
        let history = history.ok_or(FirError::TooLarge)?;
        let window = history.checked_add(block_len).ok_or(FirError::TooLarge)?;
        let outputs = output_len(layout, rate, block_len).ok_or(FirError::TooLarge)?;
        let (taps, len) = match rate {
            Rate::Decimate(_) => (taps.to_vec(), given),
            Rate::Interpolate(factor) => {
                let factor = widen(factor);
                let len = given.div_ceil(factor);
                factor.checked_mul(len).ok_or(FirError::TooLarge)?;
                (polyphase(taps, factor, len), len)
            }
        };
        Ok(Self {
            layout,
            rate,
            taps,
            len,
            window: vec![0; window],
            history,
            outputs,
        })
    }

    /// Values per input block (`filter_block_size`).
    #[must_use]
    pub fn block_len(&self) -> usize {
        self.window.len() - self.history
    }

    /// Values [`process`](Self::process) writes per block.
    ///
    /// Decimating, an output for every `2^log2` values (every `2 << log2` for
    /// [`Layout::Interleaved`], two at a time), rounding up. Interpolating, `factor` per
    /// value, or `2 * factor` per started pair interleaved.
    #[must_use]
    pub fn output_len(&self) -> usize {
        self.outputs
    }

    /// Filters one block of `input` into the first [`output_len`](Self::output_len) values
    /// of `out`, then keeps the window's last history values for the next block
    /// (`filter_data_process`, after filling `filter_data_ptr`).
    ///
    /// # Errors
    ///
    /// [`FirError::BlockLength`] when `input` is not one block, and
    /// [`FirError::OutputTooShort`] when `out` cannot hold the outputs; nothing changes.
    pub fn process(&mut self, input: &[i16], out: &mut [i16]) -> Result<(), FirError> {
        let block = self.block_len();
        if input.len() != block {
            return Err(FirError::BlockLength {
                expected: block,
                actual: input.len(),
            });
        }
        let needed = self.output_len();
        let Some(out) = out.get_mut(..needed) else {
            return Err(FirError::OutputTooShort {
                needed,
                actual: out.len(),
            });
        };
        self.window[self.history..].copy_from_slice(input);
        match (self.layout, self.rate) {
            (Layout::Real, Rate::Decimate(log2)) => self.decimate(1 << log2, out),
            (Layout::Interleaved, Rate::Decimate(log2)) => {
                self.decimate_interleaved(2 << log2, out);
            }
            (Layout::Real, Rate::Interpolate(factor)) => self.interpolate(widen(factor), out),
            (Layout::Interleaved, Rate::Interpolate(factor)) => {
                self.interpolate_interleaved(widen(factor), out);
            }
        }
        self.window.copy_within(block..block + self.history, 0);
        Ok(())
    }

    /// `conv_filter_generic`: one output per `step` values.
    fn decimate(&self, step: usize, out: &mut [i16]) {
        let positions = (0..self.block_len()).step_by(step);
        for (out, n) in out.iter_mut().zip(positions) {
            *out = narrow(dot(&self.window[n..], &self.taps), 15);
        }
    }

    /// `conv_filter_interleave_generic`: one I/Q output pair per `step` values.
    fn decimate_interleaved(&self, step: usize, out: &mut [i16]) {
        let positions = (0..self.block_len()).step_by(step);
        for (pair, n) in out.chunks_exact_mut(2).zip(positions) {
            let (i, q) = dot_pairs(&self.window[n..], &self.taps);
            pair[0] = narrow(i, 15);
            pair[1] = narrow(q, 15);
        }
    }

    /// `conv_filter_interpolate_generic`: `factor` outputs per value, one per polyphase row.
    fn interpolate(&self, factor: usize, out: &mut [i16]) {
        let shift = interpolation_shift(factor);
        for (phases, n) in out.chunks_exact_mut(factor).zip(0..self.block_len()) {
            for (out, row) in phases.iter_mut().zip(self.rows(factor)) {
                *out = narrow(dot(&self.window[n..], row), shift);
            }
        }
    }

    /// `conv_filter_interpolate_interleave_generic`: `factor` I/Q pairs per pair of values.
    fn interpolate_interleaved(&self, factor: usize, out: &mut [i16]) {
        let shift = interpolation_shift(factor);
        let positions = (0..self.block_len()).step_by(2);
        for (phases, n) in out.chunks_exact_mut(2 * factor).zip(positions) {
            for (pair, row) in phases.chunks_exact_mut(2).zip(self.rows(factor)) {
                let (i, q) = dot_pairs(&self.window[n..], row);
                pair[0] = narrow(i, shift);
                pair[1] = narrow(q, shift);
            }
        }
    }

    /// The `factor` polyphase rows, `len` taps each (empty for no taps).
    fn rows(&self, factor: usize) -> impl Iterator<Item = &[i16]> {
        (0..factor).map(move |row| &self.taps[row * self.len..(row + 1) * self.len])
    }
}

/// Why a [`Fir`] was not built or a block not filtered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FirError {
    /// A [`Rate::Decimate`] exponent above [`MAX_DECIMATION_LOG2`].
    Decimation(u32),
    /// The input is not one block.
    BlockLength {
        /// The filter's block length.
        expected: usize,
        /// The input's length.
        actual: usize,
    },
    /// The output buffer is shorter than one block's outputs.
    OutputTooShort {
        /// [`Fir::output_len`].
        needed: usize,
        /// The buffer's length.
        actual: usize,
    },
    /// The window, polyphase taps or outputs would not fit `usize`.
    TooLarge,
}

impl fmt::Display for FirError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decimation(log2) => {
                write!(f, "decimation by 2^{log2} exceeds 2^{MAX_DECIMATION_LOG2}")
            }
            Self::BlockLength { expected, actual } => {
                write!(f, "input of {actual} values is not a block of {expected}")
            }
            Self::OutputTooShort { needed, actual } => {
                write!(f, "output of {actual} values cannot hold {needed}")
            }
            Self::TooLarge => f.write_str("filter sizes exceed the address space"),
        }
    }
}

impl Error for FirError {}

/// Values one block of `block` writes, if that fits `usize` (see [`Fir::output_len`]).
fn output_len(layout: Layout, rate: Rate, block: usize) -> Option<usize> {
    match (layout, rate) {
        (Layout::Real, Rate::Decimate(log2)) => Some(block.div_ceil(1 << log2)),
        (Layout::Interleaved, Rate::Decimate(log2)) => block.div_ceil(2 << log2).checked_mul(2),
        (Layout::Real, Rate::Interpolate(factor)) => block.checked_mul(widen(factor)),
        (Layout::Interleaved, Rate::Interpolate(factor)) => {
            block.div_ceil(2).checked_mul(widen(factor))?.checked_mul(2)
        }
    }
}

/// Splits `taps` into `factor` polyphase rows of `len`, row `k` holding taps
/// `factor * i + factor - k - 1`, zero past the end.
fn polyphase(taps: &[i16], factor: usize, len: usize) -> Vec<i16> {
    (0..factor)
        .flat_map(|row| (0..len).map(move |i| factor * i + factor - row - 1))
        .map(|index| taps.get(index).copied().unwrap_or(0))
        .collect()
}

/// The interpolating kernels' output shift for `factor`.
fn interpolation_shift(factor: usize) -> u32 {
    match factor {
        1 => 15,
        2 => 14,
        4 => 13,
        8 => 12,
        16 => 11,
        _ => 10,
    }
}

/// The 32-bit, wrapping sum of `data[i] * taps[i]` over the taps.
fn dot(data: &[i16], taps: &[i16]) -> i32 {
    data.iter().zip(taps).fold(0_i32, |acc, (&d, &t)| {
        acc.wrapping_add(i32::from(d) * i32::from(t))
    })
}

/// [`dot`] for interleaved pairs: `(Σ data[2i] * taps[i], Σ data[2i + 1] * taps[i])`.
fn dot_pairs(data: &[i16], taps: &[i16]) -> (i32, i32) {
    data.chunks_exact(2)
        .zip(taps)
        .fold((0_i32, 0_i32), |(i, q), (pair, &t)| {
            (
                i.wrapping_add(i32::from(pair[0]) * i32::from(t)),
                q.wrapping_add(i32::from(pair[1]) * i32::from(t)),
            )
        })
}

/// `acc >> shift` (arithmetic), truncated to 16 bits as C's `int16_t` store is.
#[expect(
    clippy::cast_possible_truncation,
    reason = "C stores the shifted sum to int16_t, keeping its low 16 bits"
)]
fn narrow(acc: i32, shift: u32) -> i16 {
    (acc >> shift) as i16
}

/// A `u32` factor as `usize`.
fn widen(factor: NonZeroU32) -> usize {
    usize::try_from(factor.get()).expect("invariant: usize holds u32 on supported targets")
}

#[cfg(test)]
mod tests {
    use core::num::NonZeroU32;

    use super::{Fir, FirError, Layout, Rate};

    /// An interpolation factor.
    fn by(factor: u32) -> Rate {
        Rate::Interpolate(NonZeroU32::new(factor).expect("nonzero"))
    }

    /// Runs `fir` over `input` in blocks, concatenating the outputs.
    fn run(fir: &mut Fir, input: &[i16]) -> Vec<i16> {
        let mut all = Vec::new();
        for block in input.chunks_exact(fir.block_len()) {
            let mut out = vec![0; fir.output_len()];
            fir.process(block, &mut out).expect("one block");
            all.extend(out);
        }
        all
    }

    #[test]
    fn a_unit_impulse_tap_delays_by_the_history() {
        // A single 1.0 tap (32767 ≈ 1 in Q15) at index 0 outputs the window's oldest
        // value: the input from `taps` values earlier.
        let taps = [32767, 0, 0];
        let mut fir = Fir::new(&taps, Layout::Real, Rate::Decimate(0), 4).expect("valid");
        let input = [1000, 2000, 3000, 4000, 5000, 6000, 7000, 8000];
        let out = run(&mut fir, &input);
        assert_eq!(out, [0, 0, 0, 999, 1999, 2999, 3999, 4999]);
    }

    #[test]
    fn blocks_carry_history_like_one_long_block() {
        let taps: Vec<i16> = (0..7).map(|n| 3000 - 900 * n).collect();
        let input: Vec<i16> = (0..96_i16).map(|n| n.wrapping_mul(2741)).collect();
        for (layout, rate) in [
            (Layout::Real, Rate::Decimate(1)),
            (Layout::Interleaved, Rate::Decimate(1)),
            (Layout::Real, by(3)),
            (Layout::Interleaved, by(2)),
        ] {
            let small = run(
                &mut Fir::new(&taps, layout, rate, 16).expect("valid"),
                &input,
            );
            let large = run(
                &mut Fir::new(&taps, layout, rate, 48).expect("valid"),
                &input,
            );
            assert_eq!(
                small, large,
                "{layout:?} {rate:?}: block size must not matter"
            );
        }
    }

    #[test]
    fn a_block_shorter_than_the_history_moves_like_memmove() {
        // A -1.0 tap at index 0 outputs the input from five values earlier, negated.
        let taps = [i16::MIN, 0, 0, 0, 0];
        let mut fir = Fir::new(&taps, Layout::Real, Rate::Decimate(0), 2).expect("valid");
        let out = run(&mut fir, &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        assert_eq!(out, [0, 0, 0, 0, 0, -1, -2, -3, -4, -5]);
    }

    #[test]
    fn interpolation_splits_taps_into_phases_and_scales_by_the_factor() {
        // Taps h0..h3 at factor 2: rows (h1, h3) and (h0, h2), shift 14.
        let taps = [100, 200, 300, 400];
        let mut fir = Fir::new(&taps, Layout::Real, by(2), 1).expect("valid");
        let mut out = [0; 2];
        for _ in 0..5 {
            fir.process(&[16384], &mut out).expect("one block");
        }
        // The window is now full of 16384s; each row sums two of them, times its taps.
        assert_eq!(out, [200 + 400, 100 + 300]);
    }

    #[test]
    fn overflowing_sums_wrap() {
        let mut fir = Fir::new(&[i16::MIN; 2], Layout::Real, Rate::Decimate(0), 2).expect("valid");
        let mut out = [0; 2];
        fir.process(&[i16::MIN; 2], &mut out).expect("one block");
        fir.process(&[i16::MIN; 2], &mut out).expect("one block");
        // 2 * 2^30 wraps to i32::MIN; shifted by 15 that is -65536, whose low 16 bits are 0.
        assert_eq!(out, [0, 0]);
    }

    #[test]
    fn bad_shapes_are_rejected_without_effect() {
        assert_eq!(
            Fir::new(&[1], Layout::Real, Rate::Decimate(31), 4),
            Err(FirError::Decimation(31))
        );
        assert_eq!(
            Fir::new(&[1], Layout::Interleaved, by(u32::MAX), usize::MAX),
            Err(FirError::TooLarge)
        );
        let mut fir = Fir::new(&[32767], Layout::Interleaved, by(3), 3).expect("valid");
        assert_eq!(fir.output_len(), 12, "two started pairs, three phases each");
        let fresh = fir.clone();
        assert_eq!(
            fir.process(&[1, 2], &mut [0; 12]),
            Err(FirError::BlockLength {
                expected: 3,
                actual: 2
            })
        );
        assert_eq!(
            fir.process(&[1, 2, 3], &mut [0; 11]),
            Err(FirError::OutputTooShort {
                needed: 12,
                actual: 11
            })
        );
        assert_eq!(fir, fresh, "a rejected call leaves the history alone");
    }
}
