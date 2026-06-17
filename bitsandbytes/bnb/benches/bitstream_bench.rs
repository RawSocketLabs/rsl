//! Codec throughput: encode/decode through the native bit codec, over two shapes —
//! a **DMR burst** (`108|48|108` bits, fully unaligned: the general per-bit path)
//! and a **byte-aligned packet** (whole-byte fields + a `[u8; N]` payload: the
//! byte-aligned fast path, which copies whole bytes instead of shifting per bit).

use bnb::{BitDecode, BitEncode, BitEnum, u4, u48, u108};
use criterion::{Criterion, black_box, criterion_group, criterion_main};

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

criterion_group! {
    name = benches;
    config = testutil::bench::criterion();
    targets = bench_codec, bench_byte_aligned
}
criterion_main!(benches);
