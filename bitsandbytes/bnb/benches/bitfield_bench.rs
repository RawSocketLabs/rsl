//! Performance measurements for `bits`, benchmarked **against the crates it
//! replaces** (`bitbybit`, `modular-bitfield-msb`) and a hand-written
//! shift/mask baseline, on an identical 16-bit MSB-first field
//! (`a:5 | b:7 | c:4`, the DNS-header shape).
//!
//! The point is to substantiate the design claim: an integer-backed `#[bitfield]`
//! is as fast as hand-written bit twiddling, and at least on par with the
//! existing crates — so migrating to `bits` costs nothing at runtime.
//!
//! Run:        cargo bench -p bits
//! Flamegraph: cargo bench -p bits -- --profile-time 5
//! (Reports under target/criterion/.)

use criterion::{Criterion, black_box, criterion_group, criterion_main};

// --- `bits` ---------------------------------------------------------------
use bnb::{BitEnum, bitfield, u4, u5, u7};

#[bitfield(u16, bits = msb, bytes = be)]
#[derive(Clone, Copy)]
struct BitsState {
    a: u5,
    b: u7,
    c: u4,
}

// --- `bitbybit` (the dns crate's choice) ----------------------------------
mod bb {
    pub use arbitrary_int::{u4, u5, u7};
    use bitbybit::bitfield;

    #[bitfield(u16, default = 0)] // bitbybit already generates Clone/Copy
    pub struct State {
        #[bits(11..=15, rw)]
        pub a: u5,
        #[bits(4..=10, rw)]
        pub b: u7,
        #[bits(0..=3, rw)]
        pub c: u4,
    }
}

// --- `modular-bitfield-msb` (the nbt crate's choice) ----------------------
mod mb {
    use modular_bitfield_msb::prelude::*;

    #[bitfield]
    #[derive(Clone, Copy)]
    pub struct State {
        pub a: B5,
        pub b: B7,
        pub c: B4,
    }
}

fn bench_pack(c: &mut Criterion) {
    let mut g = c.benchmark_group("pack");
    g.bench_function("bits", |bn| {
        bn.iter(|| {
            BitsState::new()
                .with_a(u5::new(black_box(2)))
                .with_b(u7::new(black_box(42)))
                .with_c(u4::new(black_box(2)))
        })
    });
    g.bench_function("bitbybit", |bn| {
        bn.iter(|| {
            bb::State::default()
                .with_a(bb::u5::new(black_box(2)))
                .with_b(bb::u7::new(black_box(42)))
                .with_c(bb::u4::new(black_box(2)))
        })
    });
    g.bench_function("modular_bitfield", |bn| {
        bn.iter(|| {
            mb::State::new()
                .with_a(black_box(2))
                .with_b(black_box(42))
                .with_c(black_box(2))
        })
    });
    g.bench_function("handwritten", |bn| {
        bn.iter(|| {
            let a = black_box(2u16);
            let b = black_box(42u16);
            let c = black_box(2u16);
            ((a & 0x1F) << 11) | ((b & 0x7F) << 4) | (c & 0xF)
        })
    });
    g.finish();
}

fn bench_unpack(c: &mut Criterion) {
    let bits_s = BitsState::new()
        .with_a(u5::new(2))
        .with_b(u7::new(42))
        .with_c(u4::new(2));
    let bb_s = bb::State::default()
        .with_a(bb::u5::new(2))
        .with_b(bb::u7::new(42))
        .with_c(bb::u4::new(2));
    let mb_s = mb::State::new().with_a(2).with_b(42).with_c(2);
    let raw: u16 = 0x1002 | (42 << 4);

    let mut g = c.benchmark_group("unpack");
    g.bench_function("bits", |bn| {
        let s = black_box(bits_s);
        bn.iter(|| s.a().value() as u16 + s.b().value() as u16 + s.c().value() as u16)
    });
    g.bench_function("bitbybit", |bn| {
        let s = black_box(bb_s);
        bn.iter(|| s.a().value() as u16 + s.b().value() as u16 + s.c().value() as u16)
    });
    g.bench_function("modular_bitfield", |bn| {
        let s = black_box(mb_s);
        bn.iter(|| s.a() as u16 + s.b() as u16 + s.c() as u16)
    });
    g.bench_function("handwritten", |bn| {
        let v = black_box(raw);
        bn.iter(|| ((v >> 11) & 0x1F) + ((v >> 4) & 0x7F) + (v & 0xF))
    });
    g.finish();
}

fn bench_bytes_roundtrip(c: &mut Criterion) {
    let bits_s = BitsState::from_raw(0x12AC);
    let mb_s = mb::State::from_bytes([0x12, 0xAC]);

    let mut g = c.benchmark_group("bytes_roundtrip");
    g.bench_function("bits", |bn| {
        let s = black_box(bits_s);
        bn.iter(|| BitsState::from_be_bytes(black_box(s.to_be_bytes())))
    });
    g.bench_function("modular_bitfield", |bn| {
        let s = black_box(mb_s);
        bn.iter(|| mb::State::from_bytes(black_box(s.into_bytes())))
    });
    g.finish();
}

#[derive(BitEnum, Clone, Copy)]
#[bit_enum(u4)]
enum Code {
    A,
    B,
    C,
    #[catch_all]
    Other(u4),
}

fn bench_primitives(c: &mut Criterion) {
    let mut g = c.benchmark_group("primitives");
    g.bench_function("bits_uint_new", |bn| bn.iter(|| u5::new(black_box(17))));
    g.bench_function("arbitrary_int_new", |bn| {
        bn.iter(|| bb::u5::new(black_box(17)))
    });
    g.bench_function("bits_enum_decode", |bn| {
        bn.iter(|| <Code as bnb::Bits>::from_bits(black_box(9)))
    });
    g.bench_function("bits_enum_encode", |bn| {
        let v = black_box(Code::Other(u4::new(9)));
        bn.iter(|| <Code as bnb::Bits>::into_bits(v))
    });
    g.finish();
}

criterion_group! {
    name = benches;
    config = testutil::bench::criterion();
    targets = bench_pack, bench_unpack, bench_bytes_roundtrip, bench_primitives
}
criterion_main!(benches);
