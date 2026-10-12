//! FM modulation and quadrature demodulation of 16-bit IQ (`xdsp/fmquad.c`).
//!
//! The arithmetic is libusdr's: the modulator accumulates phase in single precision and
//! scales the carrier to 0.7 of full scale; the demodulator takes the angle between each
//! sample and its predecessor with `atan2f` over 32-bit integer products. The sine, cosine
//! and arctangent come from the platform's C math library through `std`, as in C, so
//! results match libusdr's bit for bit when both use the same library (checked against
//! glibc by the oracle's `rtsa_fm_parity` test). Where the C is undefined, this module is
//! defined instead:
//!
//! - A demodulated sample beyond `i16` (|angle × gain| ≥ 32768) or NaN converts out of
//!   range in C; here it saturates, and NaN becomes 0. A gain up to `32767 / π` cannot
//!   reach this.
//! - A sample of `(-32768, -32768)` overflows its 32-bit power, and two in a row overflow
//!   the dot product with the previous sample; both wrap here, as x86-64 builds do in
//!   practice.
//! - A non-finite phase (from a non-finite gain) yields NaN carriers, undefined to convert
//!   in C; they become 0 here.

use num_complex::Complex;

/// Carrier amplitude as a fraction of full scale.
const CARRIER: f32 = 0.7;
/// Full scale of a 16-bit sample.
const FULL_SCALE: f32 = 32767.0;

/// Frequency-modulates `audio` onto a unit carrier (`quadfm_encode`), starting at `phase`
/// radians, and returns the phase after the last sample.
///
/// Each audio sample advances the phase by `sample * gain` radians; the IQ written is
/// `(cos, sin)` of the new phase times `0.7 * 32767`, truncated. Processes as many samples
/// as both slices hold.
#[must_use = "the returned phase continues the carrier in the next block"]
pub fn modulate(audio: &[i16], gain: f32, phase: f32, iq: &mut [Complex<i16>]) -> f32 {
    let mut phase = phase;
    for (&sample, out) in audio.iter().zip(iq) {
        phase += f32::from(sample) * gain;
        let (sin, cos) = phase.sin_cos();
        *out = Complex::new(carrier(cos), carrier(sin));
    }
    phase
}

/// One carrier component: C's `(int16_t)(x * 0.7f * 32767.0f)`.
#[expect(
    clippy::cast_possible_truncation,
    reason = "|x| <= 1, so the scaled value fits; C truncates"
)]
fn carrier(x: f32) -> i16 {
    (x * CARRIER * FULL_SCALE) as i16
}

/// A quadrature FM demodulator (`quadfm_decode_state_t`): the previous sample and the
/// output gain.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Demodulator {
    /// The last sample of the previous block (`iq_prev`).
    previous: Complex<i16>,
    /// Output units per radian of phase step (`d_mp`).
    gain: f32,
}

/// Power statistics of one demodulated block.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlockPower {
    /// The largest sample power `i² + q²`, at least 0 (`omaxp`).
    pub peak: i32,
    /// The sum of the sample powers (`opwr`).
    pub total: i64,
}

impl Demodulator {
    /// A demodulator scaling each phase step by `gain` (output units per radian), with a
    /// zero previous sample; `32767 / π` maps ±π to full scale.
    #[must_use]
    pub fn new(gain: f32) -> Self {
        Self {
            previous: Complex::new(0, 0),
            gain,
        }
    }

    /// The last sample demodulated, which the next block's first sample is measured from.
    #[must_use]
    pub fn previous(&self) -> Complex<i16> {
        self.previous
    }

    /// Demodulates `iq` into `out` (`quadfm_decode`): each output is the angle of
    /// `x[n] · conj(x[n-1])`, computed as `atan2f` of the integer cross and dot products,
    /// times the gain and truncated. Returns the block's power statistics.
    ///
    /// Processes as many samples as both slices hold, and carries the last one over.
    pub fn demodulate(&mut self, iq: &[Complex<i16>], out: &mut [i16]) -> BlockPower {
        let mut power = BlockPower::default();
        let mut previous = self.previous;
        for (&sample, slot) in iq.iter().zip(out) {
            let (i, q) = (i32::from(sample.re), i32::from(sample.im));
            let (prev_i, prev_q) = (i32::from(previous.re), i32::from(previous.im));
            let sample_power = (i * i).wrapping_add(q * q);
            power.total += i64::from(sample_power);
            power.peak = power.peak.max(sample_power);
            let dot = (i * prev_i).wrapping_add(q * prev_q);
            let cross = q * prev_i - i * prev_q;
            #[expect(
                clippy::cast_precision_loss,
                reason = "C converts the int32_t products to float for atan2f"
            )]
            let angle = (cross as f32).atan2(dot as f32);
            #[expect(
                clippy::cast_possible_truncation,
                reason = "C truncates to int16_t; beyond it, saturate"
            )]
            let demodulated = (angle * self.gain) as i16;
            *slot = demodulated;
            previous = sample;
        }
        self.previous = previous;
        power
    }
}

#[cfg(test)]
mod tests {
    use num_complex::Complex;

    use super::{BlockPower, Demodulator, modulate};

    #[test]
    fn demodulation_recovers_modulated_audio() {
        let audio: Vec<i16> = (0..256_i16).map(|n| (n % 32 - 16) * 1000).collect();
        let gain = 1e-5;
        let mut iq = vec![Complex::new(0, 0); audio.len()];
        let end = modulate(&audio, gain, 0.0, &mut iq);
        let mut demodulator = Demodulator::new(1.0 / gain);
        // Prime with the carrier's starting point, (cos 0, sin 0).
        demodulator.demodulate(&[Complex::new(22_936, 0)], &mut [0]);
        let mut out = vec![0; audio.len()];
        demodulator.demodulate(&iq, &mut out);
        for (n, (&sent, &got)) in audio.iter().zip(&out).enumerate() {
            assert!(
                (i32::from(sent) - i32::from(got)).abs() <= 40,
                "sample {n}: sent {sent}, demodulated {got}"
            );
        }
        let total: f32 = audio.iter().map(|&a| f32::from(a) * gain).sum();
        assert!(
            (end - total).abs() < 1e-3,
            "returned phase {end} vs {total}"
        );
    }

    #[test]
    fn block_power_reports_peak_and_sum() {
        let mut demodulator = Demodulator::new(1.0);
        let iq = [Complex::new(3, 4), Complex::new(-6, 8), Complex::new(0, 0)];
        let power = demodulator.demodulate(&iq, &mut [0; 3]);
        assert_eq!(
            power,
            BlockPower {
                peak: 100,
                total: 125
            }
        );
        assert_eq!(demodulator.previous(), Complex::new(0, 0));
    }

    #[test]
    fn out_of_range_angles_saturate() {
        let mut demodulator = Demodulator::new(1e6);
        demodulator.demodulate(&[Complex::new(100, 0)], &mut [0]);
        let mut out = [0; 2];
        demodulator.demodulate(&[Complex::new(0, 100), Complex::new(0, -100)], &mut out);
        assert_eq!(out, [i16::MAX, i16::MAX]);
    }
}
