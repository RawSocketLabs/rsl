//! Whole-message throughput: encode (`to_bytes`) and decode (`decode_exact`) of a
//! **realistic multi-field `#[bin]` message**, end to end — not a single field.
//!
//! The message is an IPv4-header shape (the canonical sub-byte-plus-scalars protocol
//! header): a `#[bitfield]` `version`/`ihl` byte, another for the flags + 13-bit
//! fragment offset, whole-byte and 16-/32-bit scalars, two addresses, and a
//! `#[br(count)]` variable-length options/payload tail. It exercises the mix a real
//! codec hits — unaligned bitfields, byte-aligned scalars, and a var-length `Vec` — so
//! the number is an **informational baseline** for whole-message throughput, not a
//! micro-benchmark of one operation. (No CI perf gate is attached; this is a local
//! baseline only.)
//!
//! Run: cargo bench -p bitsandbytes --bench message_bench
//! (Reports under target/criterion/.)

use bnb::{BitEnum, bin, bitfield, u2, u3, u4, u6, u13};
use criterion::{Criterion, black_box, criterion_group, criterion_main};

// version:ihl — the classic IPv4 first byte (two nibbles, MSB-first).
#[bitfield(u8, bits = msb, bytes = big)]
#[derive(Clone, Copy)]
struct VersionIhl {
    version: u4,
    ihl: u4,
}

// flags (3 bits) + fragment offset (13 bits), packed MSB-first into a u16.
#[bitfield(u16, bits = msb, bytes = big)]
#[derive(Clone, Copy)]
struct FlagsFrag {
    flags: u3,
    frag_offset: u13,
}

// DSCP (6 bits) + ECN (2 bits) — the "type of service" byte, as a nested bitfield.
#[bitfield(u8, bits = msb, bytes = big)]
#[derive(Clone, Copy)]
struct Tos {
    dscp: DiffServ,
    ecn: u2,
}

// A sub-byte BitEnum nested inside the ToS bitfield (6-bit DSCP code point). Variants
// auto-number from 0; a `#[catch_all]` keeps decode total (any of the 64 code points).
#[derive(BitEnum, Clone, Copy, PartialEq, Debug)]
#[bit_enum(u6)]
enum DiffServ {
    Default,
    Cs1,
    Af11,
    Ef,
    #[catch_all]
    Other(u6),
}

/// An IPv4-header-like whole message: sub-byte bitfields, scalar fields, two addresses,
/// and a length-prefixed variable-length options/payload tail.
#[bin(big)]
#[derive(Clone)]
struct Ipv4ish {
    vi: VersionIhl,
    tos: Tos,
    total_len: u16,
    ident: u16,
    flags_frag: FlagsFrag,
    ttl: u8,
    protocol: u8,
    checksum: u16,
    src: u32,
    dst: u32,
    // Variable-length tail: a byte count then that many option/payload bytes.
    #[brw(count_prefix = u16)]
    options: Vec<u8>,
}

fn sample() -> Ipv4ish {
    Ipv4ish {
        vi: VersionIhl::new()
            .with_version(u4::new(4))
            .with_ihl(u4::new(5)),
        tos: Tos::new().with_dscp(DiffServ::Ef).with_ecn(u2::new(0)),
        total_len: 20 + 40,
        ident: 0x1C46,
        flags_frag: FlagsFrag::new()
            .with_flags(u3::new(0b010))
            .with_frag_offset(u13::new(0)),
        ttl: 64,
        protocol: 17, // UDP
        checksum: 0xB1E6,
        src: 0xC0A8_0001, // 192.168.0.1
        dst: 0xC0A8_00C8, // 192.168.0.200
        options: (0..40u8).collect(),
    }
}

fn bench_message(c: &mut Criterion) {
    let msg = sample();
    let bytes = msg.to_bytes().unwrap();

    let mut g = c.benchmark_group("ipv4ish_message");
    g.bench_function("encode", |b| {
        b.iter(|| black_box(&msg).to_bytes().unwrap());
    });
    g.bench_function("decode", |b| {
        b.iter(|| Ipv4ish::decode_exact(black_box(&bytes)).unwrap());
    });
    g.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default();
    targets = bench_message
}
criterion_main!(benches);
