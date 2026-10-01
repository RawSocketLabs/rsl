//! Seekable-file reads: `u16`/`u32` values at byte offsets through a `BufReader<File>`, the
//! container-format case (TIFF/RAW directories) — sequential walks and scattered lookups,
//! little-endian chosen at run time as a TIFF `II` header would. `SeekReader` reads through a
//! `BufReader<File>` (the shape a consumer has today); `BufSeekReader` owns the `File` and
//! supplies the same 8 KiB buffer itself.

use bnb::{BitOrder, BufSeekReader, ByteOrder, Layout, SeekReader, SeekSource};
use criterion::{Criterion, criterion_group, criterion_main};
use std::fs::File;
use std::hint::black_box;
use std::io::{BufReader, Write};
use std::path::PathBuf;

/// File size: larger than `BufReader`'s 8 KiB buffer, so scattered reads leave it.
const FILE_LEN: usize = 1 << 20;
/// Bytes walked by one sequential iteration.
const SEQ_LEN: usize = 64 * 1024;
/// Offsets visited by one scattered iteration.
const SCATTERED: usize = 4096;

/// A deterministic LCG, so every run reads the same bytes at the same offsets.
fn lcg(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *state >> 33
}

/// Writes the fixture file once and returns its path.
fn fixture() -> PathBuf {
    let path = std::env::temp_dir().join(format!("bnb-seek-bench-{}.bin", std::process::id()));
    let mut state = 0x5EED;
    let bytes: Vec<u8> = (0..FILE_LEN)
        .map(|_| lcg(&mut state).to_le_bytes()[0])
        .collect();
    File::create(&path)
        .and_then(|mut f| f.write_all(&bytes))
        .expect("invariant: the temp dir is writable");
    path
}

/// Runtime-chosen byte order, as parsed from a TIFF `II` header.
fn tiff_layout(header: [u8; 2]) -> Layout {
    let byte = if header == *b"II" {
        ByteOrder::Little
    } else {
        ByteOrder::Big
    };
    Layout {
        bit: BitOrder::Msb,
        byte,
    }
}

/// Reads `u16`, `u32` pairs from bit 0 forward through `SEQ_LEN` bytes.
fn sequential<S: SeekSource>(src: &mut S) -> u64 {
    src.seek_to_bit(0).expect("seekable");
    let mut sum = 0u64;
    for _ in 0..SEQ_LEN / 6 {
        sum += u64::from(src.read::<u16>().expect("in bounds"));
        sum += u64::from(src.read::<u32>().expect("in bounds"));
    }
    sum
}

/// Reads a `u16` and a `u32` at each precomputed byte offset.
fn scattered<S: SeekSource>(src: &mut S, offsets: &[usize]) -> u64 {
    let mut sum = 0u64;
    for &off in offsets {
        src.seek_to_bit(off * 8).expect("seekable");
        sum += u64::from(src.read::<u16>().expect("in bounds"));
        sum += u64::from(src.read::<u32>().expect("in bounds"));
    }
    sum
}

fn bench_seek(c: &mut Criterion) {
    let path = fixture();
    let layout = tiff_layout(*b"II");
    let mut state = 0xD1CE;
    let offsets: Vec<usize> = (0..SCATTERED)
        .map(|_| usize::try_from(lcg(&mut state)).expect("31-bit output fits") % (FILE_LEN - 6))
        .collect();
    let open = || BufReader::new(File::open(&path).expect("fixture exists"));

    let mut g = c.benchmark_group("seek_file");
    let mut seek = SeekReader::with_layout(open(), layout);
    g.bench_function("seek_reader/sequential", |b| {
        b.iter(|| black_box(sequential(&mut seek)));
    });
    g.bench_function("seek_reader/scattered", |b| {
        b.iter(|| black_box(scattered(&mut seek, &offsets)));
    });
    // The unbuffered alternative: one `lseek` plus a 2-4 byte `read` per value.
    let mut unbuffered =
        SeekReader::with_layout(File::open(&path).expect("fixture exists"), layout);
    g.bench_function("seek_reader_unbuffered/sequential", |b| {
        b.iter(|| black_box(sequential(&mut unbuffered)));
    });
    g.bench_function("seek_reader_unbuffered/scattered", |b| {
        b.iter(|| black_box(scattered(&mut unbuffered, &offsets)));
    });
    // Owns the `File` and its own buffer: no `BufReader` from `open`, which would double-buffer.
    let mut owned = BufSeekReader::with_layout(File::open(&path).expect("fixture exists"), layout);
    g.bench_function("buf_seek_reader/sequential", |b| {
        b.iter(|| black_box(sequential(&mut owned)));
    });
    g.bench_function("buf_seek_reader/scattered", |b| {
        b.iter(|| black_box(scattered(&mut owned, &offsets)));
    });
    g.finish();
    let _ = std::fs::remove_file(&path);
}

criterion_group! {
    name = benches;
    config = Criterion::default();
    targets = bench_seek
}
criterion_main!(benches);
