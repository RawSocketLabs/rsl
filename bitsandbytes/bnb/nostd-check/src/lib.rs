//! `no_std` compile-and-link smoke test for `bnb` (Option A).
//!
//! Built for a bare-metal target with `bnb`'s `std` feature **off**, this proves
//! the runtime *and* the macro-generated code use only `core` + `alloc` — no
//! accidental `std` linkage. It exercises every generated path that used to emit
//! `::std::…`: a `magic` constant, a `reserved` field (the canonical encode path), a
//! `count`-driven `Vec` payload, a `#[bitfield]`, and a closed `BitEnum`'s checked
//! `TryFrom` (the `UnknownDiscriminant`/`String` path).
//!
//! Run it (from the repo root):
//! ```text
//! cargo build --manifest-path bnb/nostd-check/Cargo.toml --target thumbv7em-none-eabi
//! ```
#![no_std]
#![allow(missing_docs)]

extern crate alloc;

use alloc::vec::Vec;
// `renamed_bnb` is `bnb` under a `package = "…"` alias (see Cargo.toml) — proving
// the macro-generated `::renamed_bnb::…` paths resolve via `proc-macro-crate`.
use renamed_bnb::{BitEnum, BitError, NormalizeEnumAliases, bin, bitfield, u4};

#[bitfield(u8, bits = msb)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Nibbles {
    hi: u4,
    lo: u4,
}

#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u8, closed)]
#[repr(u8)]
pub enum Kind {
    Ping = 1,
    Pong = 2,
}

// magic + reserved + a sub-byte run (so the right-tool guard passes) + a
// count-driven Vec payload.
#[bin(magic = 0x7Eu8)]
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Frame {
    tag: u4,
    #[reserved]
    rsv: u4,
    n: u8,
    #[br(count = n)]
    data: Vec<u8>,
}

/// Decode a frame from a borrowed byte slice (`core`/`alloc` only).
pub fn parse(bytes: &[u8]) -> Option<Frame> {
    Frame::decode_exact(bytes).ok()
}

/// Incremental retry/EOF through a renamed dependency, including mixed magic codegen.
#[bin(big)]
#[derive(Debug)]
pub enum Incremental {
    #[bin(magic = b"AB")]
    Long,
    #[bin(magic = b"A")]
    Short,
    Raw(u8),
}

pub fn pull_incremental(
    buffer: &mut renamed_bnb::BitBuf,
    eof: bool,
) -> Result<Option<Incremental>, BitError> {
    if eof {
        buffer.pull_eof()
    } else {
        buffer.pull()
    }
}

#[bin(read_only, ctx(count: usize))]
pub struct Directional {
    #[br(count = count)]
    bytes: Vec<u8>,
}

pub fn pull_directional(
    buffer: &mut renamed_bnb::BitBuf,
    count: usize,
) -> Result<Directional, BitError> {
    buffer.try_pull_with(renamed_bnb::Layout::default(), DirectionalCtx { count })
}

/// Encode to an owned `Vec<u8>` (alloc).
pub fn build(frame: &Frame) -> Result<Vec<u8>, BitError> {
    frame.to_bytes()
}

/// The spec-value encode path (reserved fields as spec) — exercises the canonical encode path (`to_canonical_bytes`).
pub fn build_spec(frame: &Frame) -> Result<Vec<u8>, BitError> {
    frame.to_canonical_bytes()
}

/// Checked enum conversion — exercises the `UnknownDiscriminant`/`String` path.
pub fn kind_of(value: u8) -> Option<Kind> {
    Kind::try_from(value).ok()
}

/// Bitfield pack round-trip.
pub fn pack(hi: u4, lo: u4) -> u8 {
    Nibbles::new().with_hi(hi).with_lo(lo).to_be_bytes()[0]
}

#[derive(BitEnum, Clone, Copy)]
#[bit_enum(u8)]
#[repr(u8)]
pub enum OpenKind {
    Named = 2,
    #[catch_all]
    Other(u8),
}

#[derive(renamed_bnb::BitsBuilder)]
pub struct Aliases {
    values: Option<Vec<[OpenKind; 2]>>,
    opaque: Vec<u8>,
}

pub fn normalize_aliases() -> Result<Aliases, renamed_bnb::BuilderError> {
    Aliases::builder()
        .values(Some(alloc::vec![[OpenKind::Other(2), OpenKind::Other(99)]]))
        .opaque(alloc::vec![1, 2])
        .build()
}

/// Consuming normalization on a moved, non-Clone message under a renamed dependency.
pub fn into_normalized_aliases(value: Aliases) -> Aliases {
    value.into_normalized_enum_aliases()
}
