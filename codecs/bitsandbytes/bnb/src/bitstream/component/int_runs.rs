//! Bulk counted runs of `u16`/`u32`/`u64`/`u128` (`decode_vec`/`encode_slice`) against
//! the element loops they replace: identical values, errors and final cursor for every
//! source and sink that lends an aligned run, in all four layouts, at aligned and
//! unaligned starts, for complete and truncated input.
use super::byte_runs::LAYOUTS;
use crate::bitstream::{IncrementalReader, decode_vec_each, encode_slice_each};
use crate::{BitBuf, BitDecode, BitEncode, BitReader, BitWriter, ErrorKind, Layout};
use crate::{Sink, Source};
use core::fmt::Debug;

// Byte-aligned starts and starts inside a byte, in bits.
const STARTS: [usize; 6] = [0, 1, 4, 7, 8, 24];

// Every run length through 65 elements, and a few long ones.
fn counts() -> impl Iterator<Item = usize> {
    (0..=65).chain([127, 256, 1000])
}

// Enough for 1000 `u128`s behind the latest start, and a little more.
fn data() -> Vec<u8> {
    let mut state = 0x9E37_79B9_u32;
    (0..16_100)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            state.to_be_bytes()[0]
        })
        .collect()
}

// How much input to offer a run that needs `need` bytes: more than enough, exactly
// enough, and short by one byte, by part of an element, by half, and by everything.
fn supplies(need: usize, width: usize) -> impl Iterator<Item = usize> {
    let mut lens = vec![
        need + 2,
        need,
        need.saturating_sub(1),
        need.saturating_sub(width - 1),
        need.saturating_sub(width),
        need / 2,
        0,
    ];
    lens.dedup();
    lens.into_iter()
}

fn agree_decode<T, S>(mut bulk: S, mut each: S, count: usize, case: &str)
where
    T: BitDecode + PartialEq + Debug,
    S: Source,
{
    let got = T::decode_vec(&mut bulk, count);
    assert_eq!(got, decode_vec_each::<T, _>(&mut each, count), "{case}");
    assert_eq!(bulk.bit_pos(), each.bit_pos(), "cursor: {case}");
}

// Every layout, start, count and supply for one element type, on one kind of source.
// A start the supplied input does not reach is skipped: no cursor can be put there.
fn decode_matches<'d, T, S>(d: &'d [u8], source: impl Fn(&'d [u8], Layout, usize) -> Option<S>)
where
    T: BitDecode + PartialEq + Debug,
    S: Source,
{
    let width = size_of::<T>();
    for layout in LAYOUTS {
        for start in STARTS {
            for count in counts() {
                let need = start.div_ceil(8) + count * width;
                for len in supplies(need, width) {
                    let case = format!(
                        "{} {layout:?} start {start} count {count} of {len} bytes",
                        core::any::type_name::<T>()
                    );
                    let (Some(bulk), Some(each)) = (
                        source(&d[..len], layout, start),
                        source(&d[..len], layout, start),
                    ) else {
                        continue;
                    };
                    agree_decode::<T, S>(bulk, each, count, &case);
                }
            }
        }
    }
}

// A count whose byte length overflows `usize`, and ones that merely exceed the input:
// the element path's error at the element path's position, nothing allocated up front.
fn hostile_counts_match<T: BitDecode + PartialEq + Debug>() {
    let d = &data()[..40];
    let width = size_of::<T>();
    for count in [usize::MAX, usize::MAX / width + 1, usize::MAX / width, 41] {
        for layout in LAYOUTS {
            let mk = || BitReader::with_layout(d, layout);
            let case = format!("{} count {count}", core::any::type_name::<T>());
            agree_decode::<T, _>(mk(), mk(), count, &case);
            let error = T::decode_vec(&mut mk(), count).unwrap_err();
            assert!(
                matches!(error.kind, ErrorKind::UnexpectedEof { .. }),
                "{case}"
            );
            assert_eq!(error.at, 40 / width * width * 8, "{case}");
        }
    }
}

fn reader(bytes: &[u8], layout: Layout, start: usize) -> Option<BitReader<'_>> {
    let mut r = BitReader::with_layout(bytes, layout);
    r.seek_to_bit(start).ok()?;
    Some(r)
}

macro_rules! for_each_int {
    ($check:ident $(, $arg:expr)*) => {{
        $check::<u16, _>($($arg),*);
        $check::<u32, _>($($arg),*);
        $check::<u64, _>($($arg),*);
        $check::<u128, _>($($arg),*);
    }};
}

#[test]
fn bit_reader_runs_match_per_element_reads() {
    let d = data();
    for_each_int!(decode_matches, &d, reader);
}

// A short run must stay a retry signal with the failing element's own hint.
#[test]
fn incremental_reader_runs_match_per_element_reads() {
    let d = data();
    for_each_int!(decode_matches, &d, |bytes, layout, start| {
        reader(bytes, layout, start).map(IncrementalReader)
    });
}

#[test]
fn bit_buf_runs_match_per_element_reads() {
    let d = data();
    for_each_int!(decode_matches, &d, |bytes: &[u8], layout, start| {
        let mut b = BitBuf::new().with_layout(layout);
        b.push(bytes).unwrap();
        b.seek_to_bit(start).ok()?;
        Some(b)
    });
}

// Its cursor may sit past the end, so no start is skipped here.
#[cfg(feature = "bytes")]
#[test]
fn bytes_reader_runs_match_per_element_reads() {
    use crate::BytesReader;

    let d = data();
    for_each_int!(decode_matches, &d, |bytes: &[u8], layout, start| {
        let mut r = BytesReader::with_layout(bytes::Bytes::copy_from_slice(bytes), layout);
        r.seek_to_bit(start).unwrap();
        Some(r)
    });
}

#[test]
fn hostile_counts_are_the_element_paths_eof() {
    hostile_counts_match::<u16>();
    hostile_counts_match::<u32>();
    hostile_counts_match::<u64>();
    hostile_counts_match::<u128>();
}

fn items<T: BitDecode>(count: usize) -> Vec<T> {
    let d = data();
    decode_vec_each(&mut BitReader::new(&d), count).unwrap()
}

// Writes `lead` bits first so the run lands at every alignment.
fn encode_matches<T, K>(mk: impl Fn(Layout) -> K, finish: impl Fn(K) -> Vec<u8>)
where
    T: BitDecode + BitEncode + Debug,
    K: Sink,
{
    for layout in LAYOUTS {
        for lead in [0, 1, 4, 7, 8, 24] {
            for count in counts() {
                let run = items::<T>(count);
                let case = format!(
                    "{} {layout:?} lead {lead} count {count}",
                    core::any::type_name::<T>()
                );
                let (mut bulk, mut each) = (mk(layout), mk(layout));
                bulk.write_bits(0x16_DB5A, lead).unwrap();
                each.write_bits(0x16_DB5A, lead).unwrap();
                let got = T::encode_slice(&run, &mut bulk);
                assert_eq!(got, encode_slice_each(&run, &mut each), "{case}");
                assert_eq!(bulk.bit_pos(), each.bit_pos(), "cursor: {case}");
                assert_eq!(finish(bulk), finish(each), "bytes: {case}");
            }
        }
    }
}

#[test]
fn bit_writer_runs_match_per_element_writes() {
    for_each_int!(
        encode_matches,
        BitWriter::with_layout,
        BitWriter::into_bytes
    );
}

#[cfg(feature = "bytes")]
#[test]
fn bytes_writer_runs_match_per_element_writes() {
    use crate::BytesWriter;

    for_each_int!(
        encode_matches,
        BytesWriter::with_layout,
        |w: BytesWriter| w.freeze().to_vec()
    );
}

// A run that would carry the bit position past `usize` is refused where the element
// path refuses it, before anything is appended.
fn overflowing_run_matches<T: BitDecode + BitEncode + Debug>() {
    let run = items::<T>(3);
    for layout in LAYOUTS {
        let mk = || {
            let mut w = BitWriter::with_layout(layout);
            w.bit_pos = usize::MAX & !7;
            w
        };
        let (mut bulk, mut each) = (mk(), mk());
        let got = T::encode_slice(&run, &mut bulk);
        assert_eq!(got, encode_slice_each(&run, &mut each));
        let error = got.unwrap_err();
        assert!(matches!(error.kind, ErrorKind::PositionOverflow));
        assert_eq!(error.at, usize::MAX & !7);
        assert_eq!(bulk.bit_len(), each.bit_len());
        assert_eq!(bulk.into_bytes(), [0u8; 0]);
    }
}

#[test]
fn a_run_past_the_countable_position_is_the_element_paths_error() {
    overflowing_run_matches::<u16>();
    overflowing_run_matches::<u32>();
    overflowing_run_matches::<u64>();
    overflowing_run_matches::<u128>();
}
