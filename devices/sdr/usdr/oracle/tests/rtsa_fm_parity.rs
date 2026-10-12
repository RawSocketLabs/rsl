//! `rsl_usdr::dsp::{rtsa, fm}` against libusdr's compiled `xdsp/rtsa_functions.c` (generic
//! kernels) and `xdsp/fmquad.c`, bit for bit, over repeated calls.
//!
//! Inputs stay where the C is defined (the modules' docs list where it is not):
//!
//! - RTSA charge rates stay below 0.63, so a charge step fits `uint16_t`, and
//!   `charging_frame * decay` below 2^31, so the decay shift stays below 32.
//! - FM demodulator gains stay within `32767 / π`, so outputs fit `int16_t`, and no sample
//!   is `(-32768, -32768)`, whose 32-bit power overflows.
//! - FM modulator gains and phases stay finite.
//!
//! Bitwise agreement assumes the C is not contracted into fused multiply-adds: true of the
//! oracle's baseline x86-64 build, not of FMA-capable targets (aarch64, `-march=native`).

use num_complex::{Complex, Complex32};
use rsl_usdr::dsp::fm::{self, Demodulator};
use rsl_usdr::dsp::rtsa::{Rtsa, RtsaSettings};
use rsl_usdr_oracle::fm as c_fm;
use rsl_usdr_oracle::rtsa::{self as c_rtsa, Settings};

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

    /// A random `u32` in `1..=bound`.
    fn count(&mut self, bound: u32) -> u32 {
        u32::try_from(self.below(usize::try_from(bound).expect("fits")) + 1).expect("fits")
    }

    /// A float in [-1, 1).
    fn unit(&mut self) -> f32 {
        let bits = u32::try_from(self.next() >> 41).expect("23 bits");
        f32::from_bits(0x3f80_0000 | bits) * 2.0 - 3.0
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

    /// An FFT bin: mostly a level across the display's range, sometimes an extreme.
    /// NaN only in the real part, so the energy sum meets at most one.
    fn bin(&mut self) -> [f32; 2] {
        const EXTREMES: [[f32; 2]; 8] = [
            [0.0, 0.0],
            [-0.0, 0.0],
            [f32::INFINITY, 0.0],
            [1.0, f32::NEG_INFINITY],
            [f32::NAN, 1.0],
            [f32::MAX, f32::MAX],
            [1e-30, -1e-30],
            [f32::MIN_POSITIVE, 1e-45],
        ];
        if self.below(8) == 0 {
            return EXTREMES[self.below(EXTREMES.len())];
        }
        // Magnitudes from 2^-40 to 2^20: about -240 to +120 dB.
        let exponent = i32::try_from(self.below(61)).expect("small") - 40;
        let magnitude = 2.0_f32.powi(exponent);
        [self.unit() * magnitude, self.unit() * magnitude]
    }
}

/// A valid C view of Rust's completed settings.
fn c_settings(settings: &RtsaSettings) -> Settings {
    Settings {
        upper_pwr_bound: settings.upper(),
        lower_pwr_bound: settings.lower(),
        divs_for_db: settings.divs_per_db(),
        rtsa_depth: settings.depth(),
        charging_frame: settings.charging_frame(),
        raise_coef: settings.raise(),
        decay_coef: settings.decay(),
    }
}

/// Random settings whose charge steps C defines: a charge rate below 0.63.
fn settings(rng: &mut Rng) -> RtsaSettings {
    let upper = i16::try_from(rng.below(41)).expect("small") - 20;
    let span = i16::try_from(rng.count(140)).expect("small");
    let divs = rng.count(4);
    let raise = rng.count(3);
    // charge rate = raise * divs / frame <= 0.6; frame * decay < 2^31.
    let min_frame = (raise * divs * 5).div_ceil(3);
    let extra = 1 << rng.below(12);
    let frame = min_frame + rng.count(extra) - 1;
    // Sometimes the longest decay C defines (a shift of 31), mostly a short one.
    let longest = ((1_u32 << 31) - 1) / frame;
    let decay = if rng.below(4) == 0 {
        longest
    } else {
        rng.count(64).min(longest)
    };
    RtsaSettings::new(upper, upper - span, divs, frame, raise, decay).expect("valid settings")
}

#[test]
fn rtsa_histograms_match_libusdr_bit_for_bit() {
    let mut rng = Rng(0x5eed_a001);
    let (mut cases, mut lit) = (0, 0);
    for bins in [1, 2, 3, 4, 5, 7, 8, 9, 31, 64, 100, 256] {
        for _ in 0..6 {
            let settings = settings(&mut rng);
            let mut rust = Rtsa::new(settings, bins).expect("small");
            let mut c = c_rtsa::Rtsa::new(c_settings(&settings), bins);
            assert_eq!(rust.cells(), c.cells(), "rtsa_init clears");
            let hardware = rng.below(2) == 0;
            for update in 0..40 {
                let first = rng.below(bins);
                let len = rng.below(bins - first + 1);
                if hardware {
                    let spectrum: Vec<u16> = (0..len)
                        .map(|_| u16::from_le_bytes(rng.i16().to_le_bytes()))
                        .collect();
                    let (scale, corr) = (rng.unit() / 256.0, rng.unit() * 150.0);
                    rust.update_hw(first, &spectrum, scale, corr)
                        .expect("in range");
                    c.update_hw(first, &spectrum, scale, corr);
                } else {
                    let spectrum: Vec<[f32; 2]> = (0..len).map(|_| rng.bin()).collect();
                    let bins: Vec<Complex32> = spectrum
                        .iter()
                        .map(|s| Complex32::new(s[0], s[1]))
                        .collect();
                    // Mostly 10 * log10(2), libusdr's dB scale; sometimes random.
                    let scale = if rng.below(2) == 0 {
                        3.010_3
                    } else {
                        rng.unit() * 10.0
                    };
                    let floor = [0.0, 1e-12, 1e-6, -1e-6][rng.below(4)];
                    let corr = rng.unit() * 150.0;
                    rust.update(first, &bins, scale, floor, corr)
                        .expect("in range");
                    c.update(first, &spectrum, scale, floor, corr);
                }
                assert_eq!(
                    rust.cells(),
                    c.cells(),
                    "{bins} bins, {settings:?}, update {update} of {len} bins from {first}"
                );
                cases += 1;
                lit += rust.cells().iter().filter(|&&cell| cell != 0).count();
            }
        }
    }
    assert!(cases > 2000, "every size ran repeated updates");
    assert!(lit > cases, "updates charged cells, not only empty ones");
}

#[test]
fn fm_modulation_matches_libusdr_bit_for_bit() {
    const GAINS: [f32; 6] = [0.0, 1e-5, -3e-4, 1.0, -100.0, 1e-38];
    let mut rng = Rng(0x5eed_a002);
    for len in (0..=70).chain([255, 1024, 4096]) {
        let gain = if rng.below(2) == 0 {
            GAINS[rng.below(GAINS.len())]
        } else {
            rng.unit() * 1e-3
        };
        let mut rust_phase = rng.unit() * 4.0;
        let mut c_phase = rust_phase;
        // Blocks chained through the returned phase, as a streaming caller does.
        for block in 0..3 {
            let audio: Vec<i16> = (0..len).map(|_| rng.i16()).collect();
            let mut iq = vec![Complex::new(0, 0); len];
            rust_phase = fm::modulate(&audio, gain, rust_phase, &mut iq);
            let (c_iq, end) = c_fm::modulate(&audio, gain, c_phase);
            c_phase = end;
            let rust_iq: Vec<[i16; 2]> = iq.iter().map(|s| [s.re, s.im]).collect();
            assert_eq!(rust_iq, c_iq, "{len} samples, gain {gain}, block {block}");
            assert_eq!(
                rust_phase.to_bits(),
                c_phase.to_bits(),
                "phase after {len} samples, gain {gain}, block {block}"
            );
        }
    }
}

#[test]
fn fm_demodulation_matches_libusdr_bit_for_bit() {
    // 32767 / π and below: every output fits int16_t.
    const GAINS: [f32; 5] = [10_430.0, -10_430.0, 1.0, 0.0, 5_215.189];
    let mut rng = Rng(0x5eed_a003);
    for len in (0..=70).chain([255, 1024, 4096]) {
        let gain = if rng.below(2) == 0 {
            GAINS[rng.below(GAINS.len())]
        } else {
            rng.unit() * 10_430.0
        };
        let mut rust = Demodulator::new(gain);
        let mut c = c_fm::Demodulator::new(gain);
        for block in 0..3 {
            let iq: Vec<[i16; 2]> = (0..len)
                .map(|_| match [rng.i16(), rng.i16()] {
                    [i16::MIN, i16::MIN] => [i16::MIN, i16::MIN + 1],
                    sample => sample,
                })
                .collect();
            let samples: Vec<Complex<i16>> = iq.iter().map(|s| Complex::new(s[0], s[1])).collect();
            let mut out = vec![0; len];
            let power = rust.demodulate(&samples, &mut out);
            let (c_out, peak, total) = c.demodulate(&iq);
            let what = format!("{len} samples, gain {gain}, block {block}");
            assert_eq!(out, c_out, "{what}");
            assert_eq!((power.peak, power.total), (peak, total), "{what} power");
        }
    }
}

/// Depth alignment and the rounded-down lower bound, against the C body itself.
#[test]
fn rtsa_depth_and_lower_bound_match_libusdr() {
    for (upper, lower, divs) in [
        (0_i16, -120_i16, 100_u32),
        (0, -1, 1),
        (10, -3, 3),
        (-5, -6, 7),
        (100, -100, 25),
        (0, -127, 5),
    ] {
        let rust = RtsaSettings::new(upper, lower, divs, 64, 24, 1).expect("valid settings");
        let c = rsl_usdr_oracle::rtsa::calc_depth(rsl_usdr_oracle::rtsa::Settings {
            upper_pwr_bound: upper,
            lower_pwr_bound: lower,
            divs_for_db: divs,
            rtsa_depth: 0,
            charging_frame: 64,
            raise_coef: 24,
            decay_coef: 1,
        });
        assert_eq!(
            rust.depth(),
            c.rtsa_depth,
            "depth for {upper}..{lower} at {divs}/dB"
        );
        assert_eq!(
            rust.lower(),
            c.lower_pwr_bound,
            "lower for {upper}..{lower} at {divs}/dB"
        );
    }
}
