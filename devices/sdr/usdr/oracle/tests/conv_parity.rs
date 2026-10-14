//! `usdr::dsp` against libusdr's compiled `xdsp/conv.c`: the same converters are
//! selected, their size rules agree, and every converter writes the same bytes.
//!
//! Float inputs stay where the C conversion to `int16_t` is defined: the scaled value lies
//! in (-32769, 32768). Beyond it, and for NaN and infinities, C's behaviour is undefined, so
//! no reference exists; Rust saturates there (`usdr::dsp` unit tests).

use std::ops::RangeInclusive;

use usdr::dsp::{Converter, SampleFormat};
use usdr_oracle::conv::{self, Transform};

use SampleFormat::{Cf32, Ci12, Ci16, F32, I12, I16};

/// Every format, with libusdr's name for it.
const FORMATS: [(SampleFormat, &str); 6] = [
    (I16, "i16"),
    (I12, "i12"),
    (F32, "f32"),
    (Ci16, "ci16"),
    (Ci12, "ci12"),
    (Cf32, "cf32"),
];

/// Channel counts tried, beyond libusdr's largest (8).
const CHANNELS: RangeInclusive<usize> = 0..=9;

/// Floats whose conversion C defines, at and around full scale and zero.
const EXTREME_FLOATS: [f32; 14] = [
    1.0,
    -1.0,
    0.0,
    -0.0,
    1.000_03,
    -1.000_03,
    -1.000_045,
    0.5 / 32767.0,
    -1.5 / 32767.0,
    32766.5 / 32767.0,
    1e-30,
    -1e-30,
    f32::MIN_POSITIVE,
    1e-40,
];

/// Extreme 16-bit values.
const EXTREME_I16: [i16; 5] = [i16::MIN, i16::MAX, 0, -1, 1];

/// Extreme packed bytes: all-zero, all-one and sign-boundary nibbles.
const EXTREME_BYTES: [u8; 6] = [0x00, 0xff, 0x80, 0x7f, 0xf0, 0x0f];

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

    /// `len` random bytes.
    fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next().to_le_bytes()[0]).collect()
    }

    /// `len` bytes of `format` values: random, with extremes mixed in.
    fn samples(&mut self, format: SampleFormat, len: usize) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(len + 4);
        while bytes.len() < len {
            let extreme = self.below(4) == 0;
            match format {
                F32 | Cf32 => {
                    let value = if extreme {
                        EXTREME_FLOATS[self.below(EXTREME_FLOATS.len())]
                    } else {
                        // Uniform in [-1, 1).
                        #[expect(clippy::cast_possible_truncation, reason = "random bits")]
                        let bits = self.next() as i32;
                        #[expect(clippy::cast_precision_loss, reason = "random fraction")]
                        let fraction = bits as f32 / 2_147_483_648.0;
                        fraction
                    };
                    bytes.extend(value.to_le_bytes());
                }
                I16 | Ci16 => {
                    let raw = self.next().to_le_bytes();
                    let value = if extreme {
                        EXTREME_I16[self.below(EXTREME_I16.len())]
                    } else {
                        i16::from_le_bytes([raw[0], raw[1]])
                    };
                    bytes.extend(value.to_le_bytes());
                }
                I12 | Ci12 => bytes.push(if extreme {
                    EXTREME_BYTES[self.below(EXTREME_BYTES.len())]
                } else {
                    self.next().to_le_bytes()[0]
                }),
                other => unimplemented!("no sample generator for {other:?}"),
            }
        }
        bytes.truncate(len);
        bytes
    }
}

/// Every format pair and channel shape, with both sides' selection.
fn shapes() -> Vec<(
    SampleFormat,
    &'static str,
    SampleFormat,
    &'static str,
    usize,
    usize,
)> {
    let mut all = Vec::new();
    for (from, from_name) in FORMATS {
        for (to, to_name) in FORMATS {
            for inputs in CHANNELS {
                for outputs in CHANNELS {
                    all.push((from, from_name, to, to_name, inputs, outputs));
                }
            }
        }
    }
    all
}

/// Every converter both sides select.
fn selected() -> Vec<(Converter, Transform, String, SampleFormat, usize, usize)> {
    shapes()
        .into_iter()
        .filter_map(|(from, from_name, to, to_name, inputs, outputs)| {
            let rust = Converter::new(from, to, inputs, outputs).ok()?;
            let c = conv::transform(from_name, to_name, inputs, outputs)?;
            let name = format!("{from_name}x{inputs} -> {to_name}x{outputs}");
            Some((rust, c, name, from, inputs, outputs))
        })
        .collect()
}

#[test]
fn selection_matches_libusdrs_dispatch() {
    let mut converters = 0;
    for (from, from_name, to, to_name, inputs, outputs) in shapes() {
        let rust = Converter::new(from, to, inputs, outputs);
        let shape = format!("{from_name}x{inputs} -> {to_name}x{outputs}");
        match conv::transform(from_name, to_name, inputs, outputs) {
            None => assert!(rust.is_err(), "libusdr has no {shape} converter"),
            // libusdr copies bytes for any one-to-one pair it lacks; only a format to
            // itself is a copy here.
            Some(c) if c.is_dummy() => {
                assert_eq!(
                    (inputs, outputs),
                    (1, 1),
                    "only one-to-one shapes fall back"
                );
                assert_eq!(rust.is_ok(), from == to, "{shape} is a copy only to itself");
            }
            Some(_) => {
                assert!(rust.is_ok(), "libusdr converts {shape}");
                converters += 1;
            }
        }
    }
    // libusdr's dispatch table: 12 one-to-one pair spellings, 25 split/merge shapes.
    assert_eq!(converters, 37, "every libusdr converter is selected");
}

#[test]
fn size_rules_match_libusdr() {
    let mut rng = Rng(0x5eed_0001);
    // Below 2^29, where libusdr's 32-bit arithmetic cannot wrap.
    let mut counts: Vec<u32> = (0..=1024).collect();
    counts.extend((0..1000).map(|_| u32::try_from(rng.next() % (1 << 29)).expect("< 2^29")));
    for (rust, c, name, ..) in selected() {
        for &count in &counts {
            let bytes = usize::try_from(count).expect("u32 fits usize");
            assert_eq!(
                rust.output_bytes(bytes),
                usize::try_from(c.size(count, false)).expect("u32 fits usize"),
                "{name} output size of {count} bytes"
            );
            assert_eq!(
                rust.input_bytes(bytes),
                usize::try_from(c.size(count, true)).expect("u32 fits usize"),
                "{name} input size of {count} bytes"
            );
        }
    }
}

#[test]
fn every_converter_matches_libusdr_byte_for_byte() {
    let mut rng = Rng(0x5eed_0002);
    let mut lengths: Vec<usize> = (0..=50).collect();
    lengths.extend([63, 64, 65, 95, 96, 97, 191, 192, 193, 383, 384, 385]);
    let mut overruns = 0;
    let mut cases = 0;
    for (rust, c, name, from, inputs, outputs) in selected() {
        for &input_len in &lengths {
            let total = u32::try_from(input_len * inputs).expect("small");
            let fitted = usize::try_from(c.size(total, false)).expect("u32 fits usize");
            let fitted = fitted.checked_div(outputs).unwrap_or(0);
            let random = rng.below(fitted + 9);
            for output_len in [
                fitted,
                fitted.saturating_sub(1),
                fitted + 1,
                fitted + 7,
                fitted / 2,
                0,
                random,
            ] {
                let channels: Vec<Vec<u8>> =
                    (0..inputs).map(|_| rng.samples(from, input_len)).collect();
                let views: Vec<&[u8]> = channels.iter().map(Vec::as_slice).collect();
                let initial: Vec<Vec<u8>> = (0..outputs).map(|_| rng.bytes(output_len)).collect();

                let reference = c.run(&views, &initial);
                let mut actual = initial.clone();
                let mut targets: Vec<&mut [u8]> =
                    actual.iter_mut().map(Vec::as_mut_slice).collect();
                rust.convert(&views, &mut targets)
                    .expect("buffers match the converter");

                let case = format!("{name}: {input_len} bytes in, {output_len} out per channel");
                assert_eq!(actual, reference.outputs, "{case}");
                if reference.overran {
                    // The one generic kernel that writes past a short buffer: Rust writes
                    // only what fits, which the comparison above shows is the same.
                    assert_eq!((from, inputs, outputs), (Ci12, 1, 2), "{case} overran");
                    overruns += 1;
                }
                cases += 1;
            }
        }
    }
    assert!(
        overruns > 0,
        "the ci12 split's overrunning tail is exercised"
    );
    assert!(cases > 10_000, "every converter ran across the lengths");
}
