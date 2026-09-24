// TODO(hygiene): clippy::all ratcheted off pending cleanup. This crate still uses
// the modular-bitfield/num_enum stack slated for the `bits` migration; tighten to
// the workspace default once cleaned.
#![allow(clippy::all)]

pub mod name;
pub mod session;

pub use binrw::BinWrite;
