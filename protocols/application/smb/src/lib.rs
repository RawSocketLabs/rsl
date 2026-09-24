// TODO(hygiene): clippy::all ratcheted off pending cleanup. WIP crate on the
// modular-bitfield stack slated for the `bits` migration; tighten once cleaned.
#![allow(clippy::all)]

mod shared;
pub mod v1;

mod client;

pub use client::Client;
