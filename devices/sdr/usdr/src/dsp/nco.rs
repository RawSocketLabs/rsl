//! Frequency shifting of complex 16-bit samples by a numerically controlled oscillator:
//! the port of libusdr's `xdsp/nco.c` (`nco_shift`).
//!
//! libusdr implements `nco_shift` only for SSSE3 (`do_shift_up_ssse3`); built without
//! `__SSSE3__`, its "generic" path is a stub that returns 0 and writes nothing. This is a
//! scalar port of the SSSE3 kernel's per-sample arithmetic, bit for bit, since that is the
//! only definition libusdr has. The oracle's `nco_parity` test compares it with the C when
//! the C is compiled for SSSE3 (`CFLAGS=-mssse3`), and otherwise records the stub.
//!
//! Each sample's 32-bit phase (a full turn over the `i32` range, wrapping) keeps its top 17
//! bits, folds into [-π/2, π/2) by symmetry, and feeds [`trig::sin_cos`]. The oscillator
//! multiplies the sample with Q15 rounding ([`trig`]'s `pmulhrsw` arithmetic) and wrapping
//! sums, exactly as the SSSE3 lanes do.
//!
//! One libusdr glitch is kept: a phase in [2³⁰, 2³⁰ + 2¹⁵), the first step of the second
//! quadrant, folds to -32768 rather than +32768 (which 16 bits cannot hold), so the
//! oscillator there is sin = -1 instead of +1: those samples rotate by -π/2, not +π/2.
//!
//! Where libusdr is undefined or coarser, this is defined instead:
//!
//! - libusdr works in blocks of eight samples, reading and writing up to seven samples
//!   past a count that is not a multiple of eight and advancing the phase by the whole
//!   blocks. This shifts exactly the samples given and advances the phase by exactly that
//!   many steps; for multiples of eight the two agree.
//! - libusdr's carried phase (`phase += delta << 3`) is signed overflow, undefined in C;
//!   here the phase wraps, as it does within libusdr's SIMD lanes.

use super::trig::{self, mul_round};

/// An oscillator that shifts interleaved I/Q samples in frequency, carrying its phase from
/// one call to the next.
///
/// The phase and step are fractions of a turn scaled to the `i32` range (2³² is one turn,
/// so `i32::MIN` is -π and a step of 2³⁰ is a quarter turn per sample).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Nco {
    /// The next sample's phase.
    phase: i32,
    /// The phase step per sample.
    delta: i32,
}

impl Nco {
    /// An oscillator starting at `phase`, advancing `delta` per sample.
    #[must_use]
    pub const fn new(phase: i32, delta: i32) -> Self {
        Self { phase, delta }
    }

    /// The phase the next sample will be shifted by: libusdr's return value, to pass as
    /// the next call's `inphase`.
    #[must_use]
    pub const fn phase(&self) -> i32 {
        self.phase
    }

    /// The phase step per sample.
    #[must_use]
    pub const fn delta(&self) -> i32 {
        self.delta
    }

    /// Multiplies each interleaved I/Q sample of `input` by the oscillator,
    /// `(i + jq)(cos φ + j sin φ)`, into `out` (`nco_shift`).
    ///
    /// Shifts as many whole I/Q pairs as both buffers hold; a lone trailing value and
    /// output past the shifted pairs are left untouched. The phase advances once per pair.
    pub fn shift(&mut self, input: &[i16], out: &mut [i16]) {
        for (sample, shifted) in input.chunks_exact(2).zip(out.chunks_exact_mut(2)) {
            let (cos, sin) = oscillator(self.phase);
            let (i, q) = (sample[0], sample[1]);
            shifted[0] = mul_round(cos, i).wrapping_sub(mul_round(sin, q));
            shifted[1] = mul_round(cos, q).wrapping_add(mul_round(sin, i));
            self.phase = self.phase.wrapping_add(self.delta);
        }
    }
}

/// `(cos, sin)` of a full-turn `phase` in Q15, as `calc_vco_iq_ssse3` computes one lane:
/// the top 17 bits select the angle, quadrants 1 and 2 fold back by π and negate the cosine.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "the SSSE3 lanes shift the phase's bits unsigned and keep the low 16"
)]
fn oscillator(phase: i32) -> (i16, i16) {
    let angle = (phase as u32) >> 15;
    let straight = (angle + 0x8000) & 0x1_0000 == 0;
    let low = angle as u16 as i16;
    let folded = if straight { low } else { low.wrapping_neg() };
    let (sin, cos) = trig::sin_cos(folded);
    let cos = if straight { cos } else { cos.wrapping_neg() };
    (cos, sin)
}

#[cfg(test)]
mod tests {
    use core::f64::consts::PI;

    use super::Nco;

    /// Worst error of one shifted component against the f64 rotation, in LSB: up to 4 from
    /// each of the polynomial's sine and cosine (see `trig`), 1.6 from keeping 17 phase
    /// bits, and two Q15 roundings, about 13.2 in all.
    const TOLERANCE: f64 = 14.0;

    /// The start of the phases libusdr's fold sends to -π/2 instead of +π/2.
    const GLITCH: i32 = 1 << 30;

    /// A full-scale constant input `(i, q)` shifted with `delta`, against the exact rotation.
    fn check_rotation(start: i32, delta: i32, i: i16, q: i16) {
        let samples = 4096;
        let input: Vec<i16> = [i, q].repeat(samples);
        let mut out = vec![0; input.len()];
        let mut nco = Nco::new(start, delta);
        nco.shift(&input, &mut out);
        let mut phase = start;
        for (n, pair) in out.chunks_exact(2).enumerate() {
            if (GLITCH..GLITCH + (1 << 15)).contains(&phase) {
                phase = phase.wrapping_add(delta);
                continue;
            }
            let angle = f64::from(phase) / 2_147_483_648.0 * PI;
            let (sin, cos) = angle.sin_cos();
            let (fi, fq) = (f64::from(i), f64::from(q));
            let expect_i = (fi * cos - fq * sin) * 32767.0 / 32768.0;
            let expect_q = (fq * cos + fi * sin) * 32767.0 / 32768.0;
            assert!(
                (f64::from(pair[0]) - expect_i).abs() <= TOLERANCE
                    && (f64::from(pair[1]) - expect_q).abs() <= TOLERANCE,
                "sample {n} at phase {phase}: {pair:?} vs ({expect_i}, {expect_q})"
            );
            phase = phase.wrapping_add(delta);
        }
    }

    #[test]
    fn every_quadrant_rotates_by_the_phase() {
        check_rotation(i32::MIN, 1_048_573, 32767, 0);
        check_rotation(0, -1_048_573, -20000, 25000);
        check_rotation(123_456, 7_340_033, -23_000, 23_000);
    }

    #[test]
    fn the_quarter_turn_glitch_rotates_backward_like_libusdr() {
        let mut out = [0; 4];
        Nco::new(GLITCH - 1, 1).shift(&[0, 30000, 0, 30000], &mut out);
        assert!(
            out[0] < -29000,
            "just before π/2, j * 30000j rotates to -30000"
        );
        assert!(
            out[2] > 29000,
            "at π/2 libusdr's fold gives sin = -1: +30000"
        );
    }

    #[test]
    fn split_calls_carry_the_phase() {
        let input: Vec<i16> = (0_i16..202).map(|n| n.wrapping_mul(331)).collect();
        let mut whole = vec![0; input.len()];
        Nco::new(-5, 98_765_431).shift(&input, &mut whole);

        let mut split = vec![0; input.len()];
        let mut nco = Nco::new(-5, 98_765_431);
        let (first, second) = split.split_at_mut(14);
        nco.shift(&input[..14], first);
        nco.shift(&input[14..], second);
        assert_eq!(split, whole, "two calls must equal one");
        assert_eq!(
            nco.phase(),
            (-5_i32).wrapping_add(101_i32.wrapping_mul(98_765_431)),
            "the phase advances once per pair, wrapping"
        );
    }

    #[test]
    fn only_whole_pairs_that_fit_are_shifted() {
        let mut out = [7; 5];
        let mut nco = Nco::new(0, 1 << 28);
        nco.shift(&[100, 0, 100, 0, 100, 0, 100], &mut out);
        assert_eq!(out[4], 7, "the lone trailing value is untouched");
        assert_eq!(nco.phase(), 2 << 28, "two pairs fit");
    }
}
