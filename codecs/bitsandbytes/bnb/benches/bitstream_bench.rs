//! Codec throughput: encode/decode through the native bit codec, over three shapes —
//! a **DMR burst** (`108|48|108` bits, fully unaligned: the wide unaligned path), a
//! **byte-aligned packet** (whole-byte fields + a `[u8; N]` payload: the byte-aligned fast
//! path), and a **sub-byte run** (fields of 1 to 7 bits in both bit orders: the narrow path).

use bnb::{BitDecode, BitEncode, BitEnum, u1, u2, u3, u4, u5, u6, u7, u48, u108};
use criterion::{Criterion, criterion_group};
use std::hint::black_box;

#[derive(BitEnum, Copy, Clone, Eq, PartialEq, Debug)]
#[bit_enum(u48)]
#[repr(u64)]
enum Sync {
    BaseVoice = 0x755F_D7DF_75F7,
    #[catch_all]
    Unknown(u48),
}

#[derive(BitDecode, BitEncode, Copy, Clone)]
struct Burst {
    p1: u108,
    sync: Sync,
    p2: u108,
}

#[derive(BitDecode, BitEncode, Copy, Clone)]
struct Frame {
    cc: u4,
    dt: u4,
    #[nested]
    burst: Burst,
    crc: [u8; 2],
}

fn bench_codec(c: &mut Criterion) {
    let frame = Frame {
        cc: u4::new(5),
        dt: u4::new(0xA),
        burst: Burst {
            p1: u108::from_raw(0x0123_4567_89AB_CDEF_0123_4567),
            sync: Sync::BaseVoice,
            p2: u108::from_raw(0x0FED_CBA9_8765_4321_0FED_CBA9),
        },
        crc: [0xBE, 0xEF],
    };
    let bytes = frame.to_bytes().unwrap();

    let mut g = c.benchmark_group("dmr_frame");
    g.bench_function("encode", |b| {
        b.iter(|| black_box(&frame).to_bytes().unwrap());
    });
    g.bench_function("decode", |b| {
        b.iter(|| Frame::decode_exact(black_box(&bytes)).unwrap());
    });
    g.finish();
}

/// An all-byte-aligned message: whole-byte scalar fields plus a 32-byte payload.
/// Every read/write is byte-aligned, so it exercises the fast path. (The bare derive
/// would steer this to `#[bin]`; `allow_byte_aligned` opts in for the benchmark.)
#[derive(BitDecode, BitEncode, Clone)]
#[bit_stream(allow_byte_aligned)]
struct Packet {
    version: u16,
    flags: u16,
    length: u32,
    seq: u32,
    payload: [u8; 32],
}

fn bench_byte_aligned(c: &mut Criterion) {
    let pkt = Packet {
        version: 0x0102,
        flags: 0xABCD,
        length: 0x0001_0000,
        seq: 0xDEAD_BEEF,
        payload: [0x5A; 32],
    };
    let bytes = pkt.to_bytes().unwrap();

    let mut g = c.benchmark_group("byte_aligned");
    g.bench_function("encode", |b| {
        b.iter(|| black_box(&pkt).to_bytes().unwrap());
    });
    g.bench_function("decode", |b| {
        b.iter(|| Packet::decode_exact(black_box(&bytes)).unwrap());
    });
    g.finish();
}

/// Eighteen fields of one to seven bits (72 bits): flag-and-nibble headers, where every
/// read and write is narrower than a byte and most straddle a byte boundary.
macro_rules! sub_byte_run {
    ($name:ident $(, $order:meta)?) => {
        #[derive(BitDecode, BitEncode, Copy, Clone)]
        $(#[$order])?
        struct $name {
            a: u1,
            b: u7,
            c: u3,
            d: u5,
            e: u2,
            f: u6,
            g: u4,
            h: u4,
            i: u5,
            j: u3,
            k: u7,
            l: u1,
            m: u6,
            n: u2,
            o: u3,
            p: u3,
            q: u6,
            r: u4,
        }
    };
}
sub_byte_run!(SubByteMsb);
sub_byte_run!(SubByteLsb, bit_stream(bits = lsb));

fn bench_sub_byte(c: &mut Criterion) {
    let bytes = [0xA5, 0x3C, 0x96, 0x0F, 0xF0, 0x5A, 0xC3, 0x69, 0x81];
    let mut g = c.benchmark_group("sub_byte");
    let msb = SubByteMsb::decode_exact(&bytes).unwrap();
    g.bench_function("msb_encode", |b| {
        b.iter(|| black_box(&msb).to_bytes().unwrap());
    });
    g.bench_function("msb_decode", |b| {
        b.iter(|| SubByteMsb::decode_exact(black_box(&bytes)).unwrap());
    });
    let lsb = SubByteLsb::decode_exact(&bytes).unwrap();
    g.bench_function("lsb_encode", |b| {
        b.iter(|| black_box(&lsb).to_bytes().unwrap());
    });
    g.bench_function("lsb_decode", |b| {
        b.iter(|| SubByteLsb::decode_exact(black_box(&bytes)).unwrap());
    });
    g.finish();
}

/// Numeric dispatch isolated from allocations owned by decoded payloads.
#[bnb::bin(big)]
enum NumericDispatch {
    #[bin(magic = 1u8)]
    Value(u32),
    #[bin(magic = 2u8)]
    Empty,
}

/// Fixed-byte dispatch with allocation-free successful payloads.
#[bnb::bin(big)]
enum BytesDispatch {
    #[bin(magic = b"DATA")]
    Value(u32),
    #[bin(magic = b"STOP")]
    Empty,
}

/// Measure successful, rejected, mixed, and incrementally buffered dispatch.
fn bench_dispatch(c: &mut Criterion) {
    let mut group = c.benchmark_group("enum_dispatch");
    for (name, bytes) in [
        ("integer_valid", &[1, 0, 0, 0, 7][..]),
        ("integer_invalid", &[255][..]),
    ] {
        group.bench_function(name, |b| {
            b.iter(|| black_box(NumericDispatch::decode_exact(black_box(bytes))));
        });
    }
    for (name, bytes) in [
        ("bytes_valid", &b"DATA\0\0\0\x07"[..]),
        ("bytes_invalid", &b"BAD!"[..]),
    ] {
        group.bench_function(name, |b| {
            b.iter(|| black_box(BytesDispatch::decode_exact(black_box(bytes))));
        });
    }
    group.bench_function("mixed_1_in_16_invalid", |b| {
        b.iter(|| {
            for code in 0..16 {
                black_box(NumericDispatch::decode_exact(black_box(&[
                    if code == 0 { 255 } else { 1 },
                    0,
                    0,
                    0,
                    7,
                ])))
                .ok();
            }
        });
    });
    for chunk in [1, 5, 80] {
        let corpus = [1, 0, 0, 0, 7].repeat(16);
        let mut buffer = bnb::BitBuf::bounded(80);
        group.bench_function(format!("buffered_chunk_{chunk}"), |b| {
            b.iter(|| drain(&mut buffer, black_box(&corpus), chunk));
        });
    }
    let mut buffer = bnb::BitBuf::bounded(80);
    group.bench_function("buffered_invalid", |b| {
        b.iter(|| reject(&mut buffer, black_box(&[255])));
    });
    group.finish();
}

/// The buffered unit of work. Out of line so the comparison does not depend on
/// whether a given build inlines `BitBuf::pull` into the Criterion closure.
#[inline(never)]
fn drain(buffer: &mut bnb::BitBuf, corpus: &[u8], chunk: usize) {
    for part in corpus.chunks(chunk) {
        buffer.push(part).unwrap();
        while let Some(message) = buffer.pull::<NumericDispatch>().unwrap() {
            black_box(message);
        }
    }
}

/// One terminal dispatch miss through the buffered path stream consumers use.
#[inline(never)]
fn reject(buffer: &mut bnb::BitBuf, frame: &[u8]) {
    buffer.push(frame).unwrap();
    black_box(buffer.pull::<NumericDispatch>()).ok();
    buffer.clear();
}

/// Non-timed numeric allocation probe, entered only with `BNB_DISPATCH_PROBE=1`.
#[inline(never)]
fn probe_integer(valid: bool) {
    for _ in 0..1000 {
        black_box(NumericDispatch::decode_exact(black_box(if valid {
            &[1, 0, 0, 0, 7]
        } else {
            &[255]
        })))
        .ok();
    }
}

/// Non-timed fixed-byte allocation probe; profiling never affects Criterion runs.
#[inline(never)]
fn probe_bytes(valid: bool) {
    for _ in 0..1000 {
        black_box(BytesDispatch::decode_exact(black_box(if valid {
            b"DATA\0\0\0\x07"
        } else {
            b"BAD!"
        })))
        .ok();
    }
}

/// Eight byte-string magics of mixed length (two to six bytes) with distinct first bytes:
/// dispatch tries them in declaration order, so the last variant pays for seven misses.
#[bnb::bin(big)]
enum MixedMagic {
    #[bin(magic = b"AB")]
    First(u32),
    #[bin(magic = b"CDE")]
    Second(u32),
    #[bin(magic = b"FGHI")]
    Third(u32),
    #[bin(magic = b"JK")]
    Fourth(u32),
    #[bin(magic = b"LMNOP")]
    Fifth(u32),
    #[bin(magic = b"QRS")]
    Sixth(u32),
    #[bin(magic = b"TUVWXY")]
    Seventh(u32),
    #[bin(magic = b"Z0")]
    Eighth(u32),
}

/// Mixed-width byte-string dispatch from a slice: the first declared variant against the
/// eighth. Slice only: a second `BitBuf` user in this executable changes how the buffered
/// probe below is inlined, and with it the instruction counts it exists to compare.
fn bench_mixed_magic(c: &mut Criterion) {
    let mut group = c.benchmark_group("mixed_magic");
    let first = &b"AB\0\0\0\x07"[..];
    let eighth = &b"Z0\0\0\0\x07"[..];
    for (name, bytes) in [("slice_first", first), ("slice_eighth", eighth)] {
        group.bench_function(name, |b| {
            b.iter(|| black_box(MixedMagic::decode_exact(black_box(bytes))));
        });
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default();
    targets = bench_codec, bench_byte_aligned, bench_sub_byte, bench_dispatch, bench_mixed_magic
}

/// Fixed-work buffered probe for `perf stat` instruction counts, entered only with
/// `BNB_DISPATCH_PROBE=buffered`: 1,000,000 drains of 16 messages each, fed whole frames,
/// or `BNB_DISPATCH_PROBE_CHUNK` bytes at a time (1 makes four of every five attempts fail).
#[inline(never)]
fn probe_buffered() {
    let chunk = std::env::var("BNB_DISPATCH_PROBE_CHUNK")
        .ok()
        .and_then(|chunk| chunk.parse().ok())
        .unwrap_or(5);
    let corpus = [1, 0, 0, 0, 7].repeat(16);
    let mut buffer = bnb::BitBuf::bounded(80);
    for _ in 0..1_000_000 {
        drain(&mut buffer, black_box(&corpus), chunk);
    }
}

/// Select isolated allocation accounting or ordinary uninstrumented benchmarks.
fn main() {
    if std::env::var_os("BNB_DISPATCH_PROBE").is_some_and(|mode| mode == "buffered") {
        probe_buffered();
    } else if std::env::var_os("BNB_DISPATCH_PROBE").is_some() {
        // Kept in the profiling executable, not library/test startup noise.
        #[expect(clippy::print_stdout, reason = "profiling report, not library output")]
        {
            println!(
                "BitError={} ErrorKind={} Result<NumericDispatch>={} Result<()>={}",
                size_of::<bnb::BitError>(),
                size_of::<bnb::ErrorKind>(),
                size_of::<Result<NumericDispatch, bnb::BitError>>(),
                size_of::<Result<(), bnb::BitError>>(),
            );
        }
        probe_integer(true);
        probe_integer(false);
        probe_bytes(true);
        probe_bytes(false);
    } else {
        benches();
        // What `criterion_main!` runs after its groups.
        Criterion::default().configure_from_args().final_summary();
    }
}
