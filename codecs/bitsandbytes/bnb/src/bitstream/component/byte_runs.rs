//! Bulk byte runs (`read_bytes`/`read_into`/`write_bytes`) against the per-byte loops
//! they replace: identical values, errors, partial fills, and final cursor for every
//! source and sink that overrides them, at every bit offset in all four layouts.
use crate::bitstream::{
    IncrementalReader, read_bytes_per_byte, read_into_per_byte, write_bytes_per_byte,
};
use crate::{BitBuf, BitOrder, BitReader, BitWriter, ByteOrder, Layout, Sink, Source};

pub(super) const LAYOUTS: [Layout; 4] = [
    Layout {
        bit: BitOrder::Msb,
        byte: ByteOrder::Big,
    },
    Layout {
        bit: BitOrder::Msb,
        byte: ByteOrder::Little,
    },
    Layout {
        bit: BitOrder::Lsb,
        byte: ByteOrder::Big,
    },
    Layout {
        bit: BitOrder::Lsb,
        byte: ByteOrder::Little,
    },
];
// Aligned and unaligned starts, including two whole bytes in.
const STARTS: core::ops::RangeInclusive<usize> = 0..=17;

fn data() -> Vec<u8> {
    (0u8..12).map(|i| i.wrapping_mul(37) ^ 0xA5).collect()
}

// Every layout, start, and run length up to two bytes past the end of `data`.
fn cases() -> impl Iterator<Item = (Layout, usize, usize)> {
    let len = data().len();
    LAYOUTS.into_iter().flat_map(move |layout| {
        STARTS.flat_map(move |start| (0..=len + 2).map(move |n| (layout, start, n)))
    })
}

fn agree_read_bytes<S: Source>(mut bulk: S, mut each: S, n: usize, case: &str) {
    let got = bulk.read_bytes(n);
    assert_eq!(got, read_bytes_per_byte(&mut each, n), "{case}");
    assert_eq!(bulk.bit_pos(), each.bit_pos(), "cursor: {case}");
}

fn agree_read_into<S: Source>(mut bulk: S, mut each: S, n: usize, case: &str) {
    let (mut got, mut want) = (vec![0; n], vec![0; n]);
    let result = bulk.read_into(&mut got);
    assert_eq!(result, read_into_per_byte(&mut each, &mut want), "{case}");
    assert_eq!(got, want, "partial fill: {case}");
    assert_eq!(bulk.bit_pos(), each.bit_pos(), "cursor: {case}");
}

fn reader(bytes: &[u8], layout: Layout, start: usize) -> BitReader<'_> {
    let mut r = BitReader::with_layout(bytes, layout);
    r.seek_to_bit(start).unwrap();
    r
}

#[test]
fn bit_reader_matches_per_byte_reads() {
    let d = data();
    for (layout, start, n) in cases() {
        let case = format!("{layout:?} start {start} n {n}");
        let mk = || reader(&d, layout, start);
        agree_read_bytes(mk(), mk(), n, &case);
        agree_read_into(mk(), mk(), n, &case);
    }
}

// `IncrementalReader::read_bytes` deliberately reports the whole shortfall before
// allocating (`DESIGN.md` §11.2), so only `read_into` is a pure fast path here.
#[test]
fn incremental_reader_into_matches_per_byte_reads() {
    let d = data();
    for (layout, start, n) in cases() {
        let case = format!("{layout:?} start {start} n {n}");
        let mk = || IncrementalReader(reader(&d, layout, start));
        agree_read_into(mk(), mk(), n, &case);
    }
}

#[test]
fn bit_buf_matches_per_byte_reads() {
    let d = data();
    for (layout, start, n) in cases() {
        let case = format!("{layout:?} start {start} n {n}");
        let mk = || {
            let mut b = BitBuf::new().with_layout(layout);
            b.push(&d).unwrap();
            b.seek_to_bit(start).unwrap();
            b
        };
        agree_read_bytes(mk(), mk(), n, &case);
        agree_read_into(mk(), mk(), n, &case);
    }
}

#[cfg(feature = "bytes")]
#[test]
fn bytes_reader_matches_per_byte_reads() {
    use crate::BytesReader;

    let d = bytes::Bytes::from(data());
    // A cursor seeked past the end only fails once a read touches input.
    let past_end = d.len() * 8 + 9;
    for (layout, start, n) in cases().chain([(LAYOUTS[0], past_end, 0), (LAYOUTS[0], past_end, 1)])
    {
        let case = format!("{layout:?} start {start} n {n}");
        let mk = || {
            let mut r = BytesReader::with_layout(d.clone(), layout);
            r.seek_to_bit(start).unwrap();
            r
        };
        agree_read_bytes(mk(), mk(), n, &case);
        agree_read_into(mk(), mk(), n, &case);
    }
}

// Writes `start` lead bits first so the run lands at every alignment.
fn agree_write<K: Sink>(mk: impl Fn() -> K, finish: impl Fn(K) -> Vec<u8>) {
    let d = data();
    for (_, start, n) in cases().filter(|(layout, ..)| *layout == LAYOUTS[0]) {
        let n = n.min(d.len());
        let case = format!("start {start} n {n}");
        let lead = |k: &mut K| k.write_bits(0x1_6DB5, u32::try_from(start).unwrap());
        let (mut bulk, mut each) = (mk(), mk());
        lead(&mut bulk).unwrap();
        lead(&mut each).unwrap();
        let result = bulk.write_bytes(&d[..n]);
        assert_eq!(result, write_bytes_per_byte(&mut each, &d[..n]), "{case}");
        assert_eq!(bulk.bit_pos(), each.bit_pos(), "cursor: {case}");
        assert_eq!(finish(bulk), finish(each), "bytes: {case}");
    }
}

#[test]
fn bit_writer_matches_per_byte_writes() {
    for layout in LAYOUTS {
        agree_write(|| BitWriter::with_layout(layout), BitWriter::into_bytes);
    }
}

#[cfg(feature = "bytes")]
#[test]
fn bytes_writer_matches_per_byte_writes() {
    use crate::BytesWriter;

    for layout in LAYOUTS {
        agree_write(|| BytesWriter::with_layout(layout), |w| w.freeze().to_vec());
    }
}
