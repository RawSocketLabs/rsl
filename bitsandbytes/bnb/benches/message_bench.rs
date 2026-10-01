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
//! Run: cargo bench -p bitsandbytes --bench `message_bench`
//! (Reports under target/criterion/.)

use bnb::{BitBuf, BitEnum, bin, bitfield, u2, u3, u4, u6, u13};
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;
use std::time::Duration;

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

/// A transport envelope: the body length is independent of its interpretation.
#[bin(big)]
struct Envelope {
    #[brw(count_prefix = u32)]
    body: Vec<u8>,
}

fn bench_incremental(c: &mut Criterion) {
    let mut group = c.benchmark_group("incremental_envelope");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(200));
    group.measurement_time(Duration::from_secs(1));
    for size in [64, 1024, 16 * 1024] {
        let wire = Envelope {
            body: vec![0x5a; size],
        }
        .to_bytes()
        .unwrap();
        group.throughput(Throughput::Bytes(wire.len() as u64));
        for chunk_size in [1, 64, 1500, wire.len()] {
            group.bench_with_input(
                BenchmarkId::new(size.to_string(), chunk_size),
                &chunk_size,
                |bench, &chunk_size| {
                    // Reuse the allocation between messages. Keep this harness usable
                    // against the 0.4 infallible and 0.5 fallible push APIs; an unbounded
                    // buffer has no capacity rejection in either version.
                    let mut buffer = BitBuf::with_capacity(wire.len());
                    bench.iter(|| {
                        for chunk in black_box(&wire).chunks(chunk_size) {
                            let _ = buffer.push(chunk);
                            if let Some(message) = buffer.pull::<Envelope>().unwrap() {
                                black_box(message);
                            }
                        }
                        assert!(buffer.is_empty());
                    });
                },
            );
        }
    }
    group.finish();
}

// This deliberately does NOT use the byte-blob fast path for the outer collection.
// It measures the documented replay cost of general variable-element messages.
#[bin(big)]
struct Element {
    #[brw(count_prefix = u8)]
    body: Vec<u8>,
}

#[bin(big)]
struct Collection {
    #[brw(count_prefix = u16)]
    elements: Vec<Element>,
}

fn bench_element_replay(c: &mut Criterion) {
    let mut group = c.benchmark_group("incremental_element_replay");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(200));
    group.measurement_time(Duration::from_secs(1));
    for count in [64, 1024] {
        let wire = Collection {
            elements: (0..count).map(|_| Element { body: vec![0x5a] }).collect(),
        }
        .to_bytes()
        .unwrap();
        group.throughput(Throughput::Bytes(wire.len() as u64));
        for chunk_size in [1, wire.len()] {
            group.bench_with_input(
                BenchmarkId::new(count.to_string(), chunk_size),
                &chunk_size,
                |bench, &chunk_size| {
                    let mut buffer = BitBuf::with_capacity(wire.len());
                    bench.iter(|| {
                        for chunk in black_box(&wire).chunks(chunk_size) {
                            let _ = buffer.push(chunk); // unbounded; also runs against 0.4
                            if let Some(message) = buffer.pull::<Collection>().unwrap() {
                                black_box(message);
                            }
                        }
                        assert!(buffer.is_empty());
                    });
                },
            );
        }
    }
    group.finish();
}

// Identical harness on the immutable 0.5 baseline and hinted-reader candidate.
// Reuse the transport buffer; only the decoded body allocates per iteration.
#[cfg(feature = "net")]
fn bench_stream_reads(c: &mut Criterion) {
    use std::io::{self, Read};

    struct Fragments<'a> {
        bytes: &'a [u8],
        chunk: usize,
    }
    impl Read for Fragments<'_> {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            let count = out.len().min(self.chunk);
            self.bytes.read(&mut out[..count])
        }
    }

    let mut group = c.benchmark_group("stream_envelope");
    group.sample_size(20);
    group.warm_up_time(Duration::from_millis(300));
    group.measurement_time(Duration::from_secs(2));
    for size in [64, 1024, 16 * 1024] {
        let wire = Envelope {
            body: vec![0x5a; size],
        }
        .to_bytes()
        .unwrap();
        group.throughput(Throughput::Bytes(wire.len() as u64));
        for chunk in [1, 64, wire.len()] {
            group.bench_with_input(
                BenchmarkId::new(size.to_string(), chunk),
                &chunk,
                |bench, &chunk| {
                    let mut stream =
                        bnb::MessageStream::bounded(Fragments { bytes: &[], chunk }, wire.len());
                    bench.iter(|| {
                        stream.get_mut().bytes = black_box(&wire);
                        black_box(stream.read_message::<Envelope>().unwrap());
                    });
                },
            );
        }
    }
    group.finish();
}

#[cfg(not(feature = "net"))]
fn bench_stream_reads(_: &mut Criterion) {}

criterion_group! {
    name = benches;
    config = Criterion::default();
    targets = bench_message, bench_incremental, bench_element_replay, bench_stream_reads
}
criterion_main!(benches);
