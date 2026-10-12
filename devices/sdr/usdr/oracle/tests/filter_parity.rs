//! `rsl_usdr::dsp::filter` against libusdr's compiled `xdsp/filter.c` and generic
//! `conv_filter.c` kernels: every layout and rate, edge tap and block lengths, several
//! blocks per filter so history carries, and full-scale samples.
//!
//! Inputs stay where the C is defined. Taps' absolute sum stays below 65536, so the 32-bit
//! sums cannot overflow (undefined in C; Rust wraps). Blocks are at least as long as the
//! history, so libusdr's history `memcpy` does not overlap. Interpolating shapes keep
//! within the tap buffer libusdr's `assert` guards. Beyond these, no C reference exists;
//! `rsl_usdr::dsp::filter`'s unit tests cover the defined Rust behaviour.

use std::num::NonZeroU32;

use rsl_usdr::dsp::filter::{Fir, Layout, Rate};
use rsl_usdr_oracle::filter::{self, Filter};

/// Extreme 16-bit values.
const EXTREME_I16: [i16; 5] = [i16::MIN, i16::MAX, 0, -1, 1];

/// Tap counts tried: empty, tiny, odd, around libusdr's 32-tap (cache line) rounding.
const TAP_COUNTS: [usize; 11] = [0, 1, 2, 3, 7, 8, 16, 31, 32, 33, 64];

/// Interpolation factors tried: the five with their own shift, and others.
const FACTORS: [u32; 8] = [1, 2, 3, 4, 5, 8, 16, 17];

/// Blocks run through each filter.
const BLOCKS: usize = 4;

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

    /// `len` random samples, a quarter of them extremes.
    fn samples(&mut self, len: usize) -> Vec<i16> {
        (0..len)
            .map(|_| {
                if self.below(4) == 0 {
                    EXTREME_I16[self.below(EXTREME_I16.len())]
                } else {
                    i16::from_le_bytes(self.next().to_le_bytes()[..2].try_into().expect("two"))
                }
            })
            .collect()
    }

    /// `count` random taps whose absolute sum stays below 65536; a lone tap may be
    /// `i16::MIN`.
    fn taps(&mut self, count: usize) -> Vec<i16> {
        if count == 1 {
            return vec![[i16::MIN, i16::MAX, -1][self.below(3)]];
        }
        let bound = i64::try_from(65_535 / count.max(1)).expect("small");
        let bound = bound.min(32_767);
        (0..count)
            .map(|_| {
                let span = u64::try_from(2 * bound + 1).expect("positive");
                let tap = i64::try_from(self.next() % span).expect("small") - bound;
                i16::try_from(tap).expect("within the bound")
            })
            .collect()
    }
}

/// A rate as Rust's [`Rate`] and libusdr's `decim_inter` and flags.
fn rates() -> Vec<(Rate, u32, u32)> {
    let decimate = (0..=3).map(|log2| (Rate::Decimate(log2), log2, 0));
    let interpolate = FACTORS.into_iter().map(|factor| {
        let rate = Rate::Interpolate(NonZeroU32::new(factor).expect("nonzero"));
        (rate, factor, filter::INTERPOLATE)
    });
    decimate.chain(interpolate).collect()
}

#[test]
fn every_shape_filters_like_libusdr() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut shapes = 0;
    for (layout, flags) in [(Layout::Real, 0), (Layout::Interleaved, filter::INTERLEAVE)] {
        for (rate, decim_inter, rate_flags) in rates() {
            for taps_len in TAP_COUNTS {
                if rate_flags != 0 && !filter::polyphase_fits(taps_len, decim_inter) {
                    continue;
                }
                let history = match layout {
                    Layout::Real => taps_len,
                    Layout::Interleaved => 2 * taps_len,
                };
                let mut blocks = vec![history, history + 1, history + 5, 3 * history + 2, 97];
                if history == 0 {
                    blocks.push(0);
                }
                for block in blocks.into_iter().filter(|&block| block >= history) {
                    let taps = rng.taps(taps_len);
                    let mut rust = Fir::new(&taps, layout, rate, block).expect("valid shape");
                    let mut c = Filter::new(&taps, block, decim_inter, flags | rate_flags)
                        .expect("libusdr allocates the filter");
                    let shape = format!("{layout:?} {rate:?} {taps_len} taps, block {block}");
                    assert_eq!(
                        c.block_size() as usize,
                        rust.block_len(),
                        "{shape}: block size"
                    );
                    for index in 0..BLOCKS {
                        let input = rng.samples(block);
                        let mut out = vec![0; rust.output_len()];
                        rust.process(&input, &mut out).expect("one block");
                        let run = c.process(&input);
                        assert!(!run.overran, "{shape}: C wrote past the outputs");
                        assert_eq!(run.outputs, out, "{shape}: block {index}");
                    }
                    shapes += 1;
                }
            }
        }
    }
    assert!(shapes > 1000, "the sweep covers the shapes it claims");
}

#[test]
fn libusdr_has_no_separated_layout() {
    assert!(
        Filter::new(&[1, 2, 3], 8, 0, filter::SEPARATED).is_none(),
        "libusdr rejects FDAF_SEPARATED, which rsl_usdr::dsp::filter::Layout omits"
    );
}
