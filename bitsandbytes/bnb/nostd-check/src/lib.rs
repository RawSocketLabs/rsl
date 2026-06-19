//! `no_std` compile-and-link smoke test for `bnb` (Option A).
//!
//! Built for a bare-metal target with `bnb`'s `std` feature **off**, this proves
//! the runtime *and* the macro-generated code use only `core` + `alloc` — no
//! accidental `std` linkage. It exercises every generated path that used to emit
//! `::std::…`: a `magic` constant, a `reserved` field (the `SpecEncode` path), a
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
use renamed_bnb::{BitEnum, BitError, bin, bitfield, u4};

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

/// Encode to an owned `Vec<u8>` (alloc).
pub fn build(frame: &Frame) -> Result<Vec<u8>, BitError> {
    frame.to_bytes()
}

/// The spec-value encode path (reserved fields as spec) — exercises `SpecEncode`.
pub fn build_spec(frame: &Frame) -> Result<Vec<u8>, BitError> {
    frame.to_spec_bytes()
}

/// Checked enum conversion — exercises the `UnknownDiscriminant`/`String` path.
pub fn kind_of(value: u8) -> Option<Kind> {
    Kind::try_from(value).ok()
}

/// Bitfield pack round-trip.
pub fn pack(hi: u4, lo: u4) -> u8 {
    Nibbles::new().with_hi(hi).with_lo(lo).to_be_bytes()[0]
}
