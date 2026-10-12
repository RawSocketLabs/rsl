//! Invariants of the conversions; byte-for-byte libusdr parity is `oracle/tests/conv_parity.rs`.

use super::{ConversionError, Converter, SampleFormat};

use SampleFormat::{Cf32, Ci12, Ci16, F32, I12, I16};

/// Every format.
const FORMATS: [SampleFormat; 6] = [I16, I12, F32, Ci16, Ci12, Cf32];

/// Untouched-output marker.
const SENTINEL: u8 = 0xa5;

/// Deterministic xorshift64 bytes.
fn bytes(seed: u64, len: usize) -> Vec<u8> {
    let mut state = seed;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state.to_le_bytes()[0]
        })
        .collect()
}

/// Runs `converter` from `inputs` into `outputs` fresh buffers of `len` sentinel bytes.
fn run(converter: Converter, inputs: &[&[u8]], outputs: usize, len: usize) -> Vec<Vec<u8>> {
    let mut buffers = vec![vec![SENTINEL; len]; outputs];
    let mut views: Vec<&mut [u8]> = buffers.iter_mut().map(Vec::as_mut_slice).collect();
    converter
        .convert(inputs, &mut views)
        .expect("buffers match the converter");
    buffers
}

/// Every converter the selector accepts, with its input format and shape.
fn supported() -> Vec<(Converter, SampleFormat, usize, usize)> {
    let mut all = Vec::new();
    for from in FORMATS {
        for to in FORMATS {
            for inputs in 0..=9 {
                for outputs in 0..=9 {
                    if let Ok(converter) = Converter::new(from, to, inputs, outputs) {
                        all.push((converter, from, inputs, outputs));
                    }
                }
            }
        }
    }
    all
}

/// Stored bytes of `values` values in `format`.
fn stored(format: SampleFormat, values: usize) -> usize {
    match format {
        I12 | Ci12 => values * 3 / 2,
        I16 | Ci16 => values * 2,
        F32 | Cf32 => values * 4,
    }
}

#[test]
fn splitting_then_merging_ci16_restores_the_stream() {
    for channels in [2, 3, 4] {
        let stream = bytes(7, 4 * channels * 9);
        let split = Converter::new(Ci16, Ci16, 1, channels).expect("libusdr splits ci16");
        let merge = Converter::new(Ci16, Ci16, channels, 1).expect("libusdr merges ci16");
        let parts = run(split, &[&stream], channels, stream.len() / channels);
        let views: Vec<&[u8]> = parts.iter().map(Vec::as_slice).collect();
        let merged = run(merge, &views, 1, stream.len());
        assert_eq!(merged[0], stream, "{channels} channels must round-trip");
    }
}

#[test]
fn packed_twelve_bit_survives_sixteen_bit_and_back() {
    for channels in [1, 2, 4] {
        let packed = bytes(11, 3 * channels * 7);
        let unpack = Converter::new(Ci12, Ci16, 1, channels).expect("libusdr unpacks ci12");
        let pack = Converter::new(Ci16, Ci12, channels, 1).expect("libusdr packs ci12");
        let wide = run(
            unpack,
            &[&packed],
            channels,
            packed.len() * 4 / 3 / channels,
        );
        let views: Vec<&[u8]> = wide.iter().map(Vec::as_slice).collect();
        let repacked = run(pack, &views, 1, packed.len());
        assert_eq!(repacked[0], packed, "{channels} channels must round-trip");
    }
}

#[test]
fn every_i16_survives_a_float_round_trip() {
    let values: Vec<u8> = (i16::MIN..=i16::MAX).flat_map(i16::to_le_bytes).collect();
    let to_float = Converter::new(I16, F32, 1, 1).expect("libusdr converts i16 to f32");
    let to_int = Converter::new(F32, I16, 1, 1).expect("libusdr converts f32 to i16");
    let floats = run(to_float, &[&values], 1, values.len() * 2);
    let back = run(to_int, &[&floats[0]], 1, values.len());
    assert_eq!(
        back[0], values,
        "scaling by 1/32767 and back must be lossless"
    );
}

#[test]
fn floats_beyond_full_scale_saturate_and_nan_is_zero() {
    let floats: Vec<u8> = [
        2.0_f32,
        -2.0,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
        0.5,
    ]
    .into_iter()
    .flat_map(f32::to_le_bytes)
    .collect();
    let converter = Converter::new(Cf32, Ci16, 1, 1).expect("libusdr converts cf32 to ci16");
    let out = run(converter, &[&floats], 1, 12);
    let values: Vec<i16> = out[0]
        .chunks_exact(2)
        .map(|raw| i16::from_le_bytes([raw[0], raw[1]]))
        .collect();
    assert_eq!(values, [i16::MAX, i16::MIN, i16::MAX, i16::MIN, 0, 16383]);
}

#[test]
fn whole_groups_write_exactly_the_size_rule() {
    for (converter, from, inputs, outputs) in supported() {
        // 48 values per input is a whole number of groups for every kernel.
        let input_len = stored(from, 48);
        let stream = bytes(3, input_len);
        let views = vec![stream.as_slice(); inputs];
        let total = converter.output_bytes(input_len * inputs);
        let out = run(converter, &views, outputs, total / outputs + 8);
        for buffer in &out {
            let (written, spare) = buffer.split_at(total / outputs);
            assert!(
                spare.iter().all(|&byte| byte == SENTINEL),
                "{converter:?} wrote past its size rule"
            );
            assert_ne!(written, &vec![SENTINEL; written.len()][..], "{converter:?}");
        }
    }
}

#[test]
fn a_partial_group_is_left_unconverted() {
    // Two ci16 channels: 8 input bytes per group; 4 more are a partial group.
    let converter = Converter::new(Ci16, Cf32, 1, 2).expect("libusdr splits ci16 to cf32");
    let out = run(converter, &[&[1, 0, 2, 0, 3, 0, 4, 0, 5, 0, 6, 0]], 2, 12);
    for buffer in &out {
        assert_eq!(
            &buffer[8..],
            [SENTINEL; 4],
            "the leftover sample stays unconverted"
        );
    }
}

#[test]
fn input_is_limited_to_what_the_output_holds() {
    let converter = Converter::new(I16, F32, 1, 1).expect("libusdr converts i16 to f32");
    // Room for two floats and a half: two values convert.
    let out = run(converter, &[&[1, 0, 2, 0, 3, 0, 4, 0]], 1, 10);
    assert_eq!(&out[0][8..], [SENTINEL; 2]);
    assert_eq!(
        out[0][4..8],
        (2.0_f32 / 32767.0).to_le_bytes(),
        "2 scales to 2/32767"
    );
}

#[test]
fn a_lone_last_value_takes_libusdrs_tail_layout() {
    // xdsp/templates/conv_i16_i12_generic.inc: a pair packs to three bytes; a lone last
    // value is written as its low byte, then its high byte shifted down four bits.
    let converter = Converter::new(I16, I12, 1, 1).expect("libusdr packs i16");
    let input: Vec<u8> = [0x1230_i16, 0x4560, 0x789a]
        .into_iter()
        .flat_map(i16::to_le_bytes)
        .collect();
    let out = run(converter, &[&input], 1, 6);
    assert_eq!(out[0], [0x23, 0x61, 0x45, 0x9a, 0x07, SENTINEL]);
}

#[test]
fn empty_buffers_convert_nothing() {
    for (converter, _, inputs, outputs) in supported() {
        let views = vec![&[][..]; inputs];
        let out = run(converter, &views, outputs, 0);
        assert!(out.iter().all(Vec::is_empty), "{converter:?}");
    }
}

#[test]
fn shapes_libusdr_lacks_are_rejected() {
    for (from, to, inputs, outputs) in [
        (I16, F32, 1, 2),
        (Ci16, Cf32, 1, 5),
        (Cf32, Ci12, 3, 1),
        (Ci16, Ci16, 2, 2),
        (Ci16, Ci16, 0, 1),
        // libusdr would copy bytes for these; that is no conversion.
        (Ci16, F32, 1, 1),
        (I12, Cf32, 1, 1),
    ] {
        assert_eq!(
            Converter::new(from, to, inputs, outputs),
            Err(ConversionError::Unsupported {
                from,
                to,
                inputs,
                outputs
            })
        );
    }
}

#[test]
fn mismatched_buffers_are_rejected() {
    let converter = Converter::new(Ci16, Ci16, 1, 2).expect("libusdr splits ci16");
    let mut a = [0; 8];
    let mut b = [0; 4];
    assert_eq!(
        converter.convert(&[&[0; 8]], &mut [&mut a]),
        Err(ConversionError::ChannelCount {
            expected: (1, 2),
            actual: (1, 1)
        })
    );
    assert_eq!(
        converter.convert(&[&[0; 8]], &mut [&mut a, &mut b]),
        Err(ConversionError::UnequalLengths)
    );
}
