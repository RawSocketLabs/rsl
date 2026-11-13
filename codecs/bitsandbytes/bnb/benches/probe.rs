//! Fixed-work probes for `perf stat` instruction and cycle counts: the deterministic half
//! of the performance gate (`DESIGN.md` §19, §22). Criterion timings move with code
//! placement; an instruction count per decode does not.
//!
//! Each mode is one unit of work repeated a fixed number of times. Its tier says how the
//! root `AGENTS.md` happy-path rule treats it: `happy` (valid input), `expected` (a failure
//! that is routine: `Incomplete` while streaming, dispatch fallthrough), `terminal` (a
//! rejection; recorded and bounded, not a blocker) and `encode`.
//!
//! This is its own executable so that adding a mode moves no Criterion benchmark and not
//! the `BNB_DISPATCH_PROBE=buffered` counts in `bitstream_bench`. Within it the modes share
//! monomorphized readers, as the message types of a real consumer do, so adding a mode can
//! move the others: compare two builds of the same source, and re-baseline after adding one.
//!
//! Run: `benches/probe.sh` (every mode, per-unit counts), or one mode by hand:
//! `BNB_PROBE=fixed44 perf stat -e instructions:u,cycles:u <executable>`. With no
//! `BNB_PROBE` (as under `cargo bench`) every mode runs once, as a self-check.
//! `--features net` adds the transport modes and `--features tokio` the `BinCodec` ones.
//! The gate uses the `tokio` build: it has every mode, and in it the attempt the
//! incremental and transport readers share is compiled out of line in the `prefix_*`,
//! `pull_short`, `stream_*` and `codec_decode` modes, as in a consumer (inlined, as in a
//! default-features build of this file, it hides the cost §22 row 2 found). `probe.sh`
//! checks each of those modes for the call and names any that lost it. The
//! `buffered_fixed44_*` modes inline the attempt in both builds.
//!
//! Dispatch misses (`*_invalid`, `magic_miss`) are tiered `terminal`, as the rule's
//! "rejected magic" (owner decision, §22). A consumer that probes one type after another
//! meets them routinely; the cheaper path for it is ROADMAP 0.10.x item 6, not a gate here.

use bnb::{
    BitBuf, BitDecode, BitEncode, BitEnum, BitError, Source, bin, bitfield, u1, u2, u3, u4, u5, u6,
    u7, u12, u13, u20, u28, u48, u108,
};
use std::hint::black_box;

/// A fixed-length, byte-aligned 44-byte message (`message_bench`'s `fixed_44`).
#[bin(big)]
#[derive(Clone)]
struct Fixed44 {
    kind: u16,
    flags: u16,
    seq: u32,
    ack: u32,
    src: u64,
    dst: u64,
    digest: [u8; 16],
}

/// The same fields through the bare derive, which sizes `to_bytes` differently.
#[derive(BitDecode, BitEncode, Clone)]
#[bit_stream(allow_byte_aligned)]
struct Packet {
    version: u16,
    flags: u16,
    length: u32,
    seq: u32,
    payload: [u8; 32],
}

/// Eighteen fields of one to seven bits (`bitstream_bench`'s `sub_byte`, MSB first).
#[derive(BitDecode, BitEncode, Copy, Clone)]
struct SubByte {
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

/// A 12-byte fixed record, and a counted run of them.
#[bin(big)]
#[derive(Clone)]
struct Record {
    kind: u16,
    flags: u16,
    seq: u32,
    value: u32,
}

#[bin(big)]
struct Records {
    #[brw(count_prefix = u16)]
    items: Vec<Record>,
}

#[bitfield(u8, bits = msb, bytes = big)]
#[derive(Clone, Copy)]
struct VersionIhl {
    version: u4,
    ihl: u4,
}

#[bitfield(u16, bits = msb, bytes = big)]
#[derive(Clone, Copy)]
struct FlagsFrag {
    flags: u3,
    frag_offset: u13,
}

#[bitfield(u8, bits = msb, bytes = big)]
#[derive(Clone, Copy)]
struct Tos {
    dscp: DiffServ,
    ecn: u2,
}

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

/// A fixed prefix with bitfields, then a variable-length tail (`message_bench`'s
/// `ipv4ish_message`).
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
    #[brw(count_prefix = u16)]
    options: Vec<u8>,
}

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

/// A DMR frame: wide reads that start and end inside a byte (`bitstream_bench`'s
/// `dmr_frame`).
#[derive(BitDecode, BitEncode, Copy, Clone)]
struct Frame {
    cc: u4,
    dt: u4,
    #[nested]
    burst: Burst,
    crc: [u8; 2],
}

/// Fields of 12 to 28 bits that start or end inside a byte: the unaligned read at widths
/// well under a `u128`.
#[derive(BitDecode, BitEncode, Copy, Clone)]
struct Unaligned {
    a: u4,
    b: u20,
    c: u12,
    d: u28,
}

/// The same fields with sixteen bytes after them, so every read has a full window behind
/// its first byte; alone, `Unaligned` is read entirely within a buffer's last bytes.
#[derive(BitDecode, BitEncode, Copy, Clone)]
#[bit_stream(allow_byte_aligned)]
struct UnalignedPadded {
    #[nested]
    head: Unaligned,
    pad: [u8; 16],
}

#[bin(big)]
enum NumericDispatch {
    #[bin(magic = 1u8)]
    Value(u32),
    #[bin(magic = 2u8)]
    Empty,
}

#[bin(big)]
enum BytesDispatch {
    #[bin(magic = b"DATA")]
    Value(u32),
    #[bin(magic = b"STOP")]
    Empty,
}

/// Byte-string magics of mixed length: dispatch tries them in declaration order.
#[bin(big)]
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

/// Byte-string magics where a whole message is shorter than the longest magic, so a
/// dispatch that looks at the longest magic's worth of input cannot.
#[bin(big)]
enum ShortMagic {
    #[bin(magic = b"OK")]
    Ack(u8),
    #[bin(magic = b"ERROR:")]
    Fail(u8),
    #[bin(magic = b"NO")]
    Nak(u8),
}

/// A transport envelope: a length, then that many bytes.
#[bin(big)]
struct Envelope {
    #[brw(count_prefix = u32)]
    body: Vec<u8>,
}

/// A one-byte region length, then one message that must fill that region exactly: the
/// length-delimited container `Source::limit_bytes` exists for.
fn in_region<T: BitDecode, S: Source>(r: &mut S) -> Result<T, BitError> {
    let len = usize::from(r.read::<u8>()?);
    let mut region = r.limit_bytes(len)?;
    let value = T::bit_decode(&mut region)?;
    region.finish()?;
    Ok(value)
}

/// `Fixed44` behind a window: typed reads and a `[u8; 16]` through `LimitedSource`.
#[bin(big, read_only)]
#[allow(dead_code)]
struct Limited44 {
    #[br(parse_with = in_region)]
    inner: Fixed44,
}

/// `Envelope` behind a window: a counted `Vec<u8>` run through `LimitedSource`.
#[bin(big, read_only)]
#[allow(dead_code)]
struct LimitedEnvelope {
    #[br(parse_with = in_region)]
    inner: Envelope,
}

/// `wire` behind a length byte that declares `declared` bytes.
fn region(declared: u8, wire: &[u8]) -> Vec<u8> {
    [&[declared][..], wire].concat()
}

/// One `T` from a `BytesReader` over `wire`, the reader a `bytes`-based consumer decodes
/// from. Cloning the `Bytes` is a reference-count bump, not a copy.
#[cfg(feature = "bytes")]
fn from_bytes_reader<T: BitDecode>(wire: &bytes::Bytes) -> Result<T, BitError> {
    T::bit_decode(&mut bnb::BytesReader::new(wire.clone()))
}

fn fixed44() -> Vec<u8> {
    Fixed44 {
        kind: 0x0102,
        flags: 0x8001,
        seq: 0x1C46_B1E6,
        ack: 0x0BAD_F00D,
        src: 0x0011_2233_4455_6677,
        dst: 0x8899_AABB_CCDD_EEFF,
        digest: [0x5A; 16],
    }
    .to_bytes()
    .unwrap()
}

fn packet() -> Vec<u8> {
    Packet {
        version: 0x0102,
        flags: 0xABCD,
        length: 0x0001_0000,
        seq: 0xDEAD_BEEF,
        payload: [0x5A; 32],
    }
    .to_bytes()
    .unwrap()
}

fn records() -> Vec<u8> {
    let record = Record {
        kind: 0x0102,
        flags: 0x8001,
        seq: 0x1C46_B1E6,
        value: 0x0BAD_F00D,
    };
    Records {
        items: vec![record; 1024],
    }
    .to_bytes()
    .unwrap()
}

fn ipv4ish() -> Vec<u8> {
    Ipv4ish {
        vi: VersionIhl::new()
            .with_version(u4::new(4))
            .with_ihl(u4::new(5)),
        tos: Tos::new().with_dscp(DiffServ::Ef).with_ecn(u2::new(0)),
        total_len: 60,
        ident: 0x1C46,
        flags_frag: FlagsFrag::new()
            .with_flags(u3::new(0b010))
            .with_frag_offset(u13::new(0)),
        ttl: 64,
        protocol: 17,
        checksum: 0xB1E6,
        src: 0xC0A8_0001,
        dst: 0xC0A8_00C8,
        options: (0..40u8).collect(),
    }
    .to_bytes()
    .unwrap()
}

fn frame() -> Vec<u8> {
    Frame {
        cc: u4::new(5),
        dt: u4::new(0xA),
        burst: Burst {
            p1: u108::from_raw(0x0123_4567_89AB_CDEF_0123_4567),
            sync: Sync::BaseVoice,
            p2: u108::from_raw(0x0FED_CBA9_8765_4321_0FED_CBA9),
        },
        crc: [0xBE, 0xEF],
    }
    .to_bytes()
    .unwrap()
}

/// A 68-byte envelope.
fn envelope() -> Vec<u8> {
    Envelope {
        body: vec![0x5A; 64],
    }
    .to_bytes()
    .unwrap()
}

const SUB_BYTE: [u8; 9] = [0xA5, 0x3C, 0x96, 0x0F, 0xF0, 0x5A, 0xC3, 0x69, 0x81];

#[cold]
#[inline(never)]
fn wrong_outcome() -> ! {
    panic!("a probe mode did not get the outcome it measures");
}

/// `n` runs of `op`, which must succeed or fail as the mode says. The result is consumed
/// by reference: passing an 80-byte `Result` to `black_box` by value copies it twice.
#[inline(never)]
fn repeat<R>(n: u64, ok: bool, mut op: impl FnMut() -> Result<R, BitError>) {
    for _ in 0..n {
        let result = op();
        if black_box(&result).is_ok() != ok {
            wrong_outcome();
        }
    }
}

/// `n` passes over sixteen back-to-back `Fixed44` messages, pushed `chunk` bytes at a time
/// into a bounded `BitBuf`. With `chunk` under 44 most attempts end in `Incomplete`.
#[inline(never)]
fn buffered(n: u64, chunk: usize) {
    let corpus = fixed44().repeat(16);
    let mut buffer = BitBuf::bounded(corpus.len());
    for _ in 0..n {
        let mut messages = 0;
        for part in black_box(&corpus[..]).chunks(chunk) {
            buffer.push(part).unwrap();
            while let Some(message) = buffer.pull::<Fixed44>().unwrap() {
                black_box(&message);
                messages += 1;
            }
        }
        if messages != 16 {
            wrong_outcome();
        }
    }
}

/// `n` passes over one message of each `MixedMagic` variant, pushed `chunk` bytes at a time
/// into a bounded `BitBuf`. At one byte a time the dispatch runs on a partial magic.
#[inline(never)]
fn buffered_magic(n: u64, chunk: usize) {
    let corpus = [
        &b"AB"[..],
        b"CDE",
        b"FGHI",
        b"JK",
        b"LMNOP",
        b"QRS",
        b"TUVWXY",
        b"Z0",
    ]
    .map(|magic| [magic, &[0, 0, 0, 7]].concat())
    .concat();
    let mut buffer = BitBuf::bounded(corpus.len());
    for _ in 0..n {
        let mut messages = 0;
        for part in black_box(&corpus[..]).chunks(chunk) {
            buffer.push(part).unwrap();
            while let Some(message) = buffer.pull::<MixedMagic>().unwrap() {
                black_box(&message);
                messages += 1;
            }
        }
        if messages != 8 {
            wrong_outcome();
        }
    }
}

/// `n` failed pulls of an envelope that is one byte short, the wait every streaming
/// consumer sits in.
#[inline(never)]
fn pull_short(n: u64) {
    let wire = envelope();
    let mut buffer = BitBuf::bounded(wire.len());
    buffer.push(&wire[..wire.len() - 1]).unwrap();
    for _ in 0..n {
        let pulled = black_box(&mut buffer).pull::<Envelope>();
        if !matches!(black_box(&pulled), Ok(None)) {
            wrong_outcome();
        }
    }
}

/// `n` passes of sixteen `read_message` calls over a reader that returns `chunk` bytes at
/// a time (`message_bench`'s `stream_envelope/16x40`).
#[cfg(feature = "net")]
#[inline(never)]
fn stream(n: u64, chunk: usize) {
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

    let wire = Envelope {
        body: vec![0x5A; 40],
    }
    .to_bytes()
    .unwrap()
    .repeat(16);
    let mut stream = bnb::MessageStream::bounded(Fragments { bytes: &[], chunk }, wire.len());
    for _ in 0..n {
        stream.get_mut().bytes = black_box(&wire);
        for _ in 0..16 {
            let message = stream.read_message::<Envelope>().unwrap();
            black_box(&message);
        }
    }
}

/// `n` rounds of sixteen 44-byte envelopes written through a `MessageStream` to a writer
/// that keeps nothing: the encode and the hand-over, without a socket.
#[cfg(feature = "net")]
#[inline(never)]
fn stream_write(n: u64) {
    use std::io::{self, Write};

    struct Discard(u64);
    impl Write for Discard {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0 += black_box(buf).len() as u64;
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    let message = Envelope {
        body: vec![0x5A; 40],
    };
    let mut stream = bnb::MessageStream::new(Discard(0));
    for _ in 0..n {
        for _ in 0..16 {
            stream.write_message(black_box(&message)).unwrap();
        }
    }
    if stream.get_mut().0 != n * 16 * 44 {
        wrong_outcome();
    }
}

/// `n` whole 68-byte frames through `BinCodec::decode`. Refilling the buffer (a 68-byte
/// copy) is part of each unit.
#[cfg(feature = "tokio")]
#[inline(never)]
fn codec_decode(n: u64) {
    use tokio_util::codec::Decoder;

    let wire = envelope();
    let mut codec = bnb::BinCodec::<Envelope>::new();
    let mut src = bytes::BytesMut::with_capacity(wire.len());
    for _ in 0..n {
        src.extend_from_slice(black_box(&wire));
        let decoded = codec.decode(&mut src);
        if !matches!(black_box(&decoded), Ok(Some(_))) {
            wrong_outcome();
        }
    }
}

/// `n` 44-byte messages through `BinCodec::encode` into a reused buffer.
#[cfg(feature = "tokio")]
#[inline(never)]
fn codec_encode(n: u64) {
    use tokio_util::codec::Encoder;

    let message = Fixed44::decode_exact(&fixed44()).unwrap();
    let mut codec = bnb::BinCodec::<Fixed44>::new();
    let mut dst = bytes::BytesMut::with_capacity(64);
    for _ in 0..n {
        dst.clear();
        codec.encode(black_box(&message).clone(), &mut dst).unwrap();
        black_box(&dst);
    }
}

struct Mode {
    name: &'static str,
    tier: &'static str,
    /// Repetitions in a measured run.
    runs: u64,
    /// Units of work per repetition (messages, for the buffered modes).
    units: u64,
    run: fn(u64),
}

const fn mode(name: &'static str, tier: &'static str, runs: u64, run: fn(u64)) -> Mode {
    Mode {
        name,
        tier,
        runs,
        units: 1,
        run,
    }
}

const fn per_16(mut mode: Mode) -> Mode {
    mode.units = 16;
    mode
}

const fn per_8(mut mode: Mode) -> Mode {
    mode.units = 8;
    mode
}

const M: u64 = 1_000_000;

#[rustfmt::skip]
const MODES: &[Mode] = &[
    // Valid input from a slice.
    mode("fixed44", "happy", 8 * M, |n| {
        let wire = fixed44();
        repeat(n, true, || Fixed44::decode_exact(black_box(&wire)));
    }),
    mode("packet", "happy", 8 * M, |n| {
        let wire = packet();
        repeat(n, true, || Packet::decode_exact(black_box(&wire)));
    }),
    mode("sub_byte", "happy", 4 * M, |n| {
        repeat(n, true, || SubByte::decode_exact(black_box(&SUB_BYTE)));
    }),
    mode("ipv4ish", "happy", 4 * M, |n| {
        let wire = ipv4ish();
        repeat(n, true, || Ipv4ish::decode_exact(black_box(&wire)));
    }),
    mode("dmr", "happy", 4 * M, |n| {
        let wire = frame();
        repeat(n, true, || Frame::decode_exact(black_box(&wire)));
    }),
    mode("unaligned_mid", "happy", 4 * M, |n| {
        let wire = [SUB_BYTE.as_slice(), &SUB_BYTE, &SUB_BYTE[..6]].concat();
        repeat(n, true, || UnalignedPadded::decode_exact(black_box(&wire)));
    }),
    mode("unaligned_tail", "happy", 8 * M, |n| {
        repeat(n, true, || Unaligned::decode_exact(black_box(&SUB_BYTE[..8])));
    }),
    mode("unaligned_wide_tail", "happy", 4 * M, |n| {
        // A burst alone: its last 108-bit field starts fourteen bytes from the end.
        let wire = &frame()[1..34];
        repeat(n, true, || Burst::decode_exact(black_box(wire)));
    }),
    mode("records_1024", "happy", 8_000, |n| {
        let wire = records();
        repeat(n, true, || Records::decode_exact(black_box(&wire)));
    }),
    mode("numeric_valid", "happy", 16 * M, |n| {
        repeat(n, true, || NumericDispatch::decode_exact(black_box(&[1, 0, 0, 0, 7])));
    }),
    mode("bytes_valid", "happy", 16 * M, |n| {
        repeat(n, true, || BytesDispatch::decode_exact(black_box(b"DATA\0\0\0\x07")));
    }),
    mode("magic_first", "happy", 8 * M, |n| {
        repeat(n, true, || MixedMagic::decode_exact(black_box(b"AB\0\0\0\x07")));
    }),
    // Valid input through the attempt the incremental and transport readers share.
    mode("prefix_envelope", "happy", 8 * M, |n| {
        let wire = envelope();
        repeat(n, true, || Envelope::decode_prefix(black_box(&wire)));
    }),
    mode("prefix_eof_envelope", "happy", 8 * M, |n| {
        let wire = envelope();
        repeat(n, true, || Envelope::decode_prefix_eof(black_box(&wire)));
    }),
    mode("prefix_fixed44", "happy", 8 * M, |n| {
        let wire = fixed44();
        repeat(n, true, || Fixed44::decode_prefix(black_box(&wire)));
    }),
    per_16(mode("buffered_fixed44_whole", "happy", M / 2, |n| buffered(n, 44))),
    // Failures a working consumer meets all the time.
    mode("magic_eighth", "expected", 4 * M, |n| {
        repeat(n, true, || MixedMagic::decode_exact(black_box(b"Z0\0\0\0\x07")));
    }),
    per_16(mode("buffered_fixed44_5", "expected", M / 8, |n| buffered(n, 5))),
    per_16(mode("buffered_fixed44_1", "expected", M / 32, |n| buffered(n, 1))),
    mode("pull_short", "expected", 8 * M, pull_short),
    mode("prefix_short_envelope", "expected", 8 * M, |n| {
        let wire = envelope();
        repeat(n, false, || Envelope::decode_prefix(black_box(&wire[..67])));
    }),
    mode("prefix_short_fixed44", "expected", 4 * M, |n| {
        let wire = fixed44();
        repeat(n, false, || Fixed44::decode_prefix(black_box(&wire[..43])));
    }),
    // Rejections: recorded and bounded, not blockers.
    mode("numeric_invalid", "terminal", 8 * M, |n| {
        repeat(n, false, || NumericDispatch::decode_exact(black_box(&[255])));
    }),
    mode("bytes_invalid", "terminal", 8 * M, |n| {
        repeat(n, false, || BytesDispatch::decode_exact(black_box(b"BAD!")));
    }),
    mode("magic_miss", "terminal", 2 * M, |n| {
        repeat(n, false, || MixedMagic::decode_exact(black_box(b"??\0\0\0\x07")));
    }),
    mode("fixed44_short", "terminal", 2 * M, |n| {
        let wire = fixed44();
        repeat(n, false, || Fixed44::decode_exact(black_box(&wire[..43])));
    }),
    mode("fixed44_trailing", "terminal", 4 * M, |n| {
        let mut wire = fixed44();
        wire.push(0);
        repeat(n, false, || Fixed44::decode_exact(black_box(&wire)));
    }),
    // Mixed-width magic dispatch through `BitBuf`, and where the input is shorter than
    // the longest magic.
    per_8(mode("buffered_magic_whole", "happy", M / 2, |n| buffered_magic(n, 59))),
    per_8(mode("buffered_magic_1", "expected", M / 8, |n| buffered_magic(n, 1))),
    mode("magic_short_first", "happy", 8 * M, |n| {
        repeat(n, true, || ShortMagic::decode_exact(black_box(b"OK\x07")));
    }),
    mode("magic_short_third", "expected", 8 * M, |n| {
        repeat(n, true, || ShortMagic::decode_exact(black_box(b"NO\x07")));
    }),
    // A bounded region (`Source::limit_bytes`): every read goes through `LimitedSource`.
    mode("limit_fixed44", "happy", 4 * M, |n| {
        let wire = region(44, &fixed44());
        repeat(n, true, || Limited44::decode_exact(black_box(&wire)));
    }),
    mode("limit_prefix_fixed44", "happy", 4 * M, |n| {
        let wire = region(44, &fixed44());
        repeat(n, true, || Limited44::decode_prefix(black_box(&wire)));
    }),
    mode("limit_envelope", "happy", 2 * M, |n| {
        let wire = region(68, &envelope());
        repeat(n, true, || LimitedEnvelope::decode_exact(black_box(&wire)));
    }),
    mode("limit_prefix_envelope", "happy", 2 * M, |n| {
        let wire = region(68, &envelope());
        repeat(n, true, || LimitedEnvelope::decode_prefix(black_box(&wire)));
    }),
    mode("limit_prefix_short_fixed44", "expected", 2 * M, |n| {
        let wire = region(44, &fixed44());
        repeat(n, false, || Limited44::decode_prefix(black_box(&wire[..44])));
    }),
    mode("limit_prefix_short_envelope", "expected", 2 * M, |n| {
        let wire = region(68, &envelope());
        repeat(n, false, || LimitedEnvelope::decode_prefix(black_box(&wire[..68])));
    }),
    mode("limit_fixed44_short", "terminal", 2 * M, |n| {
        // The region ends one byte before the message does.
        let wire = region(43, &fixed44()[..43]);
        repeat(n, false, || Limited44::decode_exact(black_box(&wire)));
    }),
    mode("limit_fixed44_long", "terminal", 2 * M, |n| {
        // The region holds one byte more than the message: not consumed exactly.
        let mut wire = region(45, &fixed44());
        wire.push(0);
        repeat(n, false, || Limited44::decode_exact(black_box(&wire)));
    }),
    // Encode.
    mode("fixed44_encode", "encode", 4 * M, |n| {
        let message = Fixed44::decode_exact(&fixed44()).unwrap();
        repeat(n, true, || black_box(&message).to_bytes());
    }),
    mode("packet_encode", "encode", 2 * M, |n| {
        let message = Packet::decode_exact(&packet()).unwrap();
        repeat(n, true, || black_box(&message).to_bytes());
    }),
    mode("ipv4ish_encode", "encode", 2 * M, |n| {
        let message = Ipv4ish::decode_exact(&ipv4ish()).unwrap();
        repeat(n, true, || black_box(&message).to_bytes());
    }),
    mode("dmr_encode", "encode", M, |n| {
        let message = Frame::decode_exact(&frame()).unwrap();
        repeat(n, true, || black_box(&message).to_bytes());
    }),
    // Variable-length encodes: the output is sized from the message, not known up front.
    mode("envelope_encode", "encode", 4 * M, |n| {
        let message = Envelope::decode_exact(&envelope()).unwrap();
        repeat(n, true, || black_box(&message).to_bytes());
    }),
    mode("records_encode", "encode", 8_000, |n| {
        let message = Records::decode_exact(&records()).unwrap();
        repeat(n, true, || black_box(&message).to_bytes());
    }),
    #[cfg(feature = "net")]
    per_16(mode("stream_16x40_whole", "happy", M / 4, |n| stream(n, 704))),
    #[cfg(feature = "net")]
    per_16(mode("stream_16x40_64", "expected", M / 4, |n| stream(n, 64))),
    #[cfg(feature = "net")]
    per_16(mode("stream_16x40_1", "expected", M / 32, |n| stream(n, 1))),
    #[cfg(feature = "net")]
    per_16(mode("stream_write_16x40", "encode", M / 4, stream_write)),
    #[cfg(feature = "bytes")]
    mode("bytes_reader_fixed44", "happy", 4 * M, |n| {
        let wire = bytes::Bytes::from(fixed44());
        repeat(n, true, || from_bytes_reader::<Fixed44>(black_box(&wire)));
    }),
    #[cfg(feature = "bytes")]
    mode("bytes_reader_sub_byte", "happy", 4 * M, |n| {
        let wire = bytes::Bytes::from_static(&SUB_BYTE);
        repeat(n, true, || from_bytes_reader::<SubByte>(black_box(&wire)));
    }),
    #[cfg(feature = "bytes")]
    mode("bytes_reader_envelope", "happy", 4 * M, |n| {
        let wire = bytes::Bytes::from(envelope());
        repeat(n, true, || from_bytes_reader::<Envelope>(black_box(&wire)));
    }),
    #[cfg(feature = "tokio")]
    mode("codec_decode", "happy", 8 * M, codec_decode),
    #[cfg(feature = "tokio")]
    mode("codec_encode", "encode", 4 * M, codec_encode),
];

#[expect(
    clippy::print_stdout,
    reason = "the probe's report, read by benches/probe.sh"
)]
fn main() {
    let Some(wanted) = std::env::var_os("BNB_PROBE") else {
        // `cargo bench` and `cargo test --benches`: check every mode still gets its outcome.
        for mode in MODES {
            (mode.run)(1);
        }
        println!(
            "{} probe modes ok; set BNB_PROBE=<mode> or run benches/probe.sh",
            MODES.len()
        );
        return;
    };
    if wanted == "list" {
        for mode in MODES {
            println!("{} {} {}", mode.name, mode.tier, mode.runs * mode.units);
        }
        return;
    }
    let Some(mode) = MODES.iter().find(|mode| wanted == mode.name) else {
        panic!("unknown BNB_PROBE mode; BNB_PROBE=list prints them");
    };
    (mode.run)(mode.runs);
}
