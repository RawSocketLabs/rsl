//! `usdr::dsp::{window, fftad}` against libusdr's compiled generic kernels
//! (`xdsp/fft_window_functions.c`, `xdsp/fftad_functions.c`), bit for bit.
//!
//! NaN appears in at most one operand of any C operation: with two, which payload x86
//! propagates depends on the compiler's operand order, which neither language fixes.
//!
//! Bitwise agreement assumes the C is not contracted into fused multiply-adds: true of the
//! oracle's baseline x86-64 build, not of FMA-capable targets (aarch64, `-march=native`).

use num_complex::{Complex, Complex32};
use usdr::dsp::fftad::{HwLogPowerAccumulator, LogPowerAccumulator};
use usdr::dsp::window;
use usdr_oracle::fft::{self as c, Accumulator};

/// Deterministic xorshift64.
struct Rng(u64);

impl Rng {
    /// The next 64 random bits.
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A random index below `bound`.
    fn below(&mut self, bound: usize) -> usize {
        usize::try_from(self.next() % bound as u64).expect("below a usize bound")
    }

    /// A float in [-1, 1).
    fn unit(&mut self) -> f32 {
        let bits = u32::try_from(self.next() >> 41).expect("23 bits");
        f32::from_bits(0x3f80_0000 | bits) * 2.0 - 3.0
    }

    /// A float of random sign, magnitude and mantissa, finite or not.
    fn wild(&mut self) -> f32 {
        f32::from_bits(u32::try_from(self.next() >> 32).expect("32 bits"))
    }

    /// A sample value: mostly moderate, with extremes and random bit patterns mixed in.
    /// NaN only when `nan` allows it.
    fn sample(&mut self, nan: bool) -> f32 {
        const EXTREMES: [f32; 12] = [
            0.0,
            -0.0,
            1.0,
            -1.0,
            f32::MAX,
            f32::MIN,
            f32::MIN_POSITIVE,
            1e-40,
            -1e-45,
            f32::INFINITY,
            f32::NEG_INFINITY,
            1e19,
        ];
        let value = match self.below(8) {
            0 => EXTREMES[self.below(EXTREMES.len())],
            1 => self.wild(),
            _ => self.unit() * 32768.0,
        };
        if value.is_nan() && !nan { 0.5 } else { value }
    }

    /// A 16-bit value, extremes mixed in.
    fn i16(&mut self) -> i16 {
        const EXTREMES: [i16; 5] = [i16::MIN, i16::MAX, 0, -1, 1];
        if self.below(4) == 0 {
            EXTREMES[self.below(EXTREMES.len())]
        } else {
            i16::from_le_bytes(self.next().to_le_bytes()[..2].try_into().expect("2 bytes"))
        }
    }

    /// An unsigned 16-bit value, extremes mixed in.
    fn u16(&mut self) -> u16 {
        u16::from_le_bytes(self.i16().to_le_bytes())
    }
}

/// Every size tried: all small ones, powers of two and their neighbours.
fn sizes() -> Vec<usize> {
    let mut sizes: Vec<usize> = (0..=70).collect();
    sizes.extend([127, 128, 129, 255, 256, 1000, 1024, 4096]);
    sizes
}

/// Bit patterns, so NaNs and signed zeros compare exactly.
fn bits(values: &[f32]) -> Vec<u32> {
    values.iter().map(|v| v.to_bits()).collect()
}

/// Complex bit patterns.
fn complex_bits(values: &[Complex32]) -> Vec<[u32; 2]> {
    values
        .iter()
        .map(|v| [v.re.to_bits(), v.im.to_bits()])
        .collect()
}

/// libusdr's pairs as bit patterns.
fn pair_bits(values: &[[f32; 2]]) -> Vec<[u32; 2]> {
    values
        .iter()
        .map(|v| [v[0].to_bits(), v[1].to_bits()])
        .collect()
}

#[test]
fn windowing_matches_libusdr_bit_for_bit() {
    let mut rng = Rng(0x5eed_f001);
    for len in sizes() {
        for _ in 0..4 {
            // NaN in the samples only: each product then has at most one.
            let window: Vec<f32> = (0..len).map(|_| rng.sample(false)).collect();
            let samples: Vec<[f32; 2]> = (0..len)
                .map(|_| [rng.sample(true), rng.sample(true)])
                .collect();
            let ints: Vec<[i16; 2]> = (0..len).map(|_| [rng.i16(), rng.i16()]).collect();

            let rust_in: Vec<Complex32> =
                samples.iter().map(|s| Complex32::new(s[0], s[1])).collect();
            let mut out = vec![Complex32::new(0.0, 0.0); len];
            window::apply(&rust_in, &window, &mut out);
            assert_eq!(
                complex_bits(&out),
                pair_bits(&c::window_cf32(&samples, &window)),
                "cf32 window of {len} samples"
            );

            let rust_in: Vec<Complex<i16>> =
                ints.iter().map(|s| Complex::new(s[0], s[1])).collect();
            window::apply_ci16(&rust_in, &window, &mut out);
            assert_eq!(
                complex_bits(&out),
                pair_bits(&c::window_ci16(&ints, &window)),
                "ci16 window of {len} samples"
            );
        }
    }
}

#[test]
fn log_power_accumulation_matches_libusdr_bit_for_bit() {
    const FLOORS: [f32; 6] = [0.0, 1e-12, 1e-3, 1.0, -1.0, 1e30];
    let mut rng = Rng(0x5eed_f002);
    for len in sizes() {
        let floor = FLOORS[rng.below(FLOORS.len())];
        let mut rust = LogPowerAccumulator::new(len, floor);
        let mut c = Accumulator::new(len, floor);
        for round in 0..3 {
            for add in 0..=rng.below(12) {
                // NaN in the real part only: the energy sum then has at most one.
                let spectrum: Vec<[f32; 2]> = (0..len)
                    .map(|_| [rng.sample(true), rng.sample(false)])
                    .collect();
                let bins: Vec<Complex32> = spectrum
                    .iter()
                    .map(|s| Complex32::new(s[0], s[1]))
                    .collect();
                rust.add(&bins);
                c.add(&spectrum);

                let (scale, corr) = (rng.unit() * 10.0, rng.unit() * 200.0);
                let initial: Vec<f32> = (0..len).map(|_| rng.unit()).collect();
                let mut out = initial.clone();
                rust.normalize(scale, corr, &mut out);
                assert_eq!(
                    bits(&out),
                    bits(&c.normalize(scale, corr, &initial)),
                    "{len} bins, floor {floor}, round {round}, after add {add}"
                );
            }
            rust.reset();
            c.init();
        }
    }
}

#[test]
fn hardware_accumulation_matches_libusdr_bit_for_bit() {
    let mut rng = Rng(0x5eed_f003);
    for len in sizes() {
        let mut rust = HwLogPowerAccumulator::new(len);
        let mut c = Accumulator::new_hw(len);
        for round in 0..2 {
            // Enough full-scale additions to pass 2^24, where float sums start rounding.
            for add in 0..300 {
                let spectrum: Vec<u16> = (0..len).map(|_| rng.u16()).collect();
                rust.add(&spectrum);
                c.add_hw(&spectrum);
                if add % 37 != 0 {
                    continue;
                }
                let (scale, corr) = (rng.unit() / 1024.0, rng.unit() * 200.0);
                let initial: Vec<f32> = (0..len).map(|_| rng.unit()).collect();
                let mut out = initial.clone();
                rust.normalize(scale, corr, &mut out);
                assert_eq!(
                    bits(&out),
                    bits(&c.normalize(scale, corr, &initial)),
                    "{len} bins, round {round}, after add {add}"
                );
            }
            rust.reset();
            c.init();
        }
    }
}
