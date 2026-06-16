//! Codec throughput (Phase 1): encode/decode a DMR-frame-shaped message through
//! the native bit codec. Establishes a baseline; the head-to-head vs. the binrw
//! path comes with Phase 2/4 (ROADMAP).

use bits::{BitDecode, BitEncode, BitEnum, u4, u48, u108};
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

criterion_group! {
    name = benches;
    config = testutil::bench::criterion();
    targets = bench_codec
}
criterion_main!(benches);
