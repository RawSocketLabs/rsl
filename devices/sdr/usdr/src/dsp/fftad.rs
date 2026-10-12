//! Averaging FFT power spectra in the log domain (`xdsp/fftad_functions.c` and its generic
//! templates `fftad_{init,add,norm}{,_hwi16}_generic.t`).
//!
//! [`LogPowerAccumulator`] multiplies each bin's power across FFTs, keeping the running
//! product as a float mantissa in [1, 2) and a separate integer power of two, so it cannot
//! overflow. Its normalization takes the base-2 logarithm of that product and scales it, so
//! `scale` is typically `10 * log10(2) / count` to give the mean power in dB.
//! [`HwLogPowerAccumulator`] sums spectra that the gateware already delivered as
//! fixed-point logarithms.
//!
//! The arithmetic is libusdr's generic kernels, bit for bit, including its bit-level float
//! surgery (checked against the compiled C by the oracle's `fft_parity` test). The SIMD
//! variants are not ported.

use num_complex::Complex32;

use super::log2::polylog2;

/// IEEE single-precision exponent bias.
const EXPONENT_BIAS: i32 = 127;
/// Everything but the exponent field: sign and mantissa (C's `fnoexp`).
const NO_EXPONENT: u32 = !(0xff << 23);
/// The exponent field of 1.0 (C's `fexp0`).
const EXPONENT_ZERO: u32 = 127 << 23;

/// A per-bin product of FFT powers, held as mantissa and exponent: libusdr's
/// `fft_accumulate_data` driven by `fftad_init`, `fftad_add` and `fftad_norm`.
#[derive(Clone, Debug, PartialEq)]
pub struct LogPowerAccumulator {
    /// Each bin's product mantissa (`f_mant`).
    mantissas: Vec<f32>,
    /// Each bin's product exponent (`f_pwr`).
    exponents: Vec<i32>,
    /// Power added to every bin before it is multiplied in, keeping silent bins away from
    /// a zero product (`mine`).
    floor: f32,
}

impl LogPowerAccumulator {
    /// An accumulator for `bins`-point spectra, emptied as `fftad_init` does, that adds
    /// `floor` to each bin's power.
    #[must_use]
    pub fn new(bins: usize, floor: f32) -> Self {
        Self {
            mantissas: vec![1.0; bins],
            exponents: vec![0; bins],
            floor,
        }
    }

    /// The spectrum size this accumulates.
    #[must_use]
    pub fn bins(&self) -> usize {
        self.mantissas.len()
    }

    /// Empties the accumulator (`fftad_init`): every product becomes 1.
    pub fn reset(&mut self) {
        self.mantissas.fill(1.0);
        self.exponents.fill(0);
    }

    /// Multiplies one spectrum in (`fftad_add`): each bin's `re² + im² + floor`.
    ///
    /// The new mantissa keeps the product's sign and mantissa bits under a zero exponent,
    /// and the product's sign-and-exponent bits, less the bias, join the running exponent.
    /// As in libusdr, a negative product (from a negative floor) thus carries its sign bit
    /// into the exponent, and infinities and NaN add their all-ones exponent. Processes as
    /// many bins as both hold.
    pub fn add(&mut self, spectrum: &[Complex32]) {
        let products = self.mantissas.iter_mut().zip(&mut self.exponents);
        for ((mantissa, exponent), bin) in products.zip(spectrum) {
            let energy = bin.re * bin.re + bin.im * bin.im;
            let product = (energy + self.floor) * *mantissa;
            let bits = product.to_bits();
            *mantissa = f32::from_bits((bits & NO_EXPONENT) | EXPONENT_ZERO);
            // C: (int32_t)(bits >> 23); the sum is int32_t and could overflow only after
            // millions of additions, which C leaves undefined.
            #[expect(clippy::cast_possible_wrap, reason = "at most 9 bits")]
            let sign_and_exponent = (bits >> 23) as i32;
            *exponent = exponent
                .wrapping_add(sign_and_exponent)
                .wrapping_sub(EXPONENT_BIAS);
        }
    }

    /// Writes each bin's `scale * log2(product) + corr` (`fftad_norm`), with the
    /// logarithm approximated as libusdr does, swapping the spectrum's halves: bin `i` goes
    /// to `out[i ^ (bins / 2)]`, which puts zero frequency in the middle.
    ///
    /// For a size that is not a power of two, libusdr writes some bins past the end of
    /// `out`; those are dropped here, and their slots are left untouched. Bins whose slot
    /// lies beyond a short `out` are dropped too.
    pub fn normalize(&self, scale: f32, corr: f32, out: &mut [f32]) {
        let half = self.bins() / 2;
        for (bin, (&mantissa, &exponent)) in self.mantissas.iter().zip(&self.exponents).enumerate()
        {
            #[expect(
                clippy::cast_precision_loss,
                reason = "C converts the int32_t exponent to float"
            )]
            let log = exponent as f32 + polylog2(mantissa);
            if let Some(slot) = out.get_mut(bin ^ half) {
                *slot = scale * log + corr;
            }
        }
    }
}

/// A per-bin sum of hardware log-power spectra: libusdr's `fft_accumulate_data` driven by
/// `fftad_init_hwi16`, `fftad_add_hwi16` and `fftad_norm_hwi16`.
///
/// The gateware's spectra are 16-bit fixed-point logarithms, already in output order, so
/// averaging them is a plain sum, and normalization is linear and unshifted.
#[derive(Clone, Debug, PartialEq)]
pub struct HwLogPowerAccumulator {
    /// Each bin's running sum (`f_mant`).
    sums: Vec<f32>,
}

impl HwLogPowerAccumulator {
    /// An accumulator for `bins`-point spectra, emptied as `fftad_init_hwi16` does.
    #[must_use]
    pub fn new(bins: usize) -> Self {
        Self {
            sums: vec![0.0; bins],
        }
    }

    /// The spectrum size this accumulates.
    #[must_use]
    pub fn bins(&self) -> usize {
        self.sums.len()
    }

    /// Empties the accumulator (`fftad_init_hwi16`): every sum becomes 0.
    pub fn reset(&mut self) {
        self.sums.fill(0.0);
    }

    /// Adds one spectrum in single precision (`fftad_add_hwi16`). Processes as many bins
    /// as both hold.
    pub fn add(&mut self, spectrum: &[u16]) {
        for (sum, &bin) in self.sums.iter_mut().zip(spectrum) {
            *sum += f32::from(bin);
        }
    }

    /// Writes each bin's `scale * sum + corr` to the same index (`fftad_norm_hwi16`).
    /// Processes as many bins as both hold.
    pub fn normalize(&self, scale: f32, corr: f32, out: &mut [f32]) {
        for (slot, &sum) in out.iter_mut().zip(&self.sums) {
            *slot = scale * sum + corr;
        }
    }
}

#[cfg(test)]
mod tests {
    use num_complex::Complex32;

    use super::{HwLogPowerAccumulator, LogPowerAccumulator};

    #[test]
    fn mean_power_of_constant_spectra_is_their_power() {
        let mut acc = LogPowerAccumulator::new(4, 0.0);
        let bins = [
            Complex32::new(1.0, 0.0),
            Complex32::new(3.0, 4.0),
            Complex32::new(0.5, 0.5),
            Complex32::new(1024.0, 0.0),
        ];
        for _ in 0..8 {
            acc.add(&bins);
        }
        let mut out = [0.0; 4];
        acc.normalize(1.0 / 8.0, 0.0, &mut out);
        // Halves swapped: bin i is at i ^ 2.
        let expected = [0.0, 25.0_f32.log2(), 0.5_f32.log2(), 20.0];
        for (bin, &power) in expected.iter().enumerate() {
            let got = out[bin ^ 2];
            assert!((got - power).abs() < 1e-3, "bin {bin}: {got} vs {power}");
        }
    }

    #[test]
    fn products_beyond_float_range_do_not_overflow() {
        let mut acc = LogPowerAccumulator::new(1, 0.0);
        for _ in 0..1000 {
            acc.add(&[Complex32::new(1e18, 0.0)]);
        }
        let mut out = [0.0];
        acc.normalize(1.0 / 1000.0, 0.0, &mut out);
        let expected = 1e36_f32.log2();
        assert!((out[0] - expected).abs() < 1e-2, "{} vs {expected}", out[0]);
    }

    #[test]
    fn odd_sizes_drop_bins_libusdr_writes_past_the_end() {
        let acc = LogPowerAccumulator::new(5, 0.0);
        let mut out = [f32::NAN; 5];
        acc.normalize(1.0, 7.0, &mut out);
        // Bins 0..=4 go to 2, 3, 0, 1 and 6; slot 4 is never written.
        assert_eq!(out[..4], [7.0; 4]);
        assert!(out[4].is_nan());
    }

    #[test]
    fn reset_restores_a_fresh_accumulator() {
        let mut acc = LogPowerAccumulator::new(3, 0.25);
        acc.add(&[Complex32::new(2.0, -1.0); 3]);
        acc.reset();
        assert_eq!(acc, LogPowerAccumulator::new(3, 0.25));

        let mut hw = HwLogPowerAccumulator::new(3);
        hw.add(&[1, 2, 3]);
        hw.reset();
        assert_eq!(hw, HwLogPowerAccumulator::new(3));
    }

    #[test]
    fn hardware_spectra_sum_and_scale_in_place() {
        let mut acc = HwLogPowerAccumulator::new(3);
        acc.add(&[1, 2, u16::MAX]);
        acc.add(&[3, 4, u16::MAX]);
        let mut out = [0.0; 3];
        acc.normalize(0.5, -1.0, &mut out);
        assert_eq!(out, [1.0, 2.0, 65534.0]);
    }
}
