//! `#[bin]`, the unified codec attribute: its options, its entry point, and one front-end
//! per item kind (`structs`, `enums`).

mod args;
mod enums;
mod expand;
mod item;
mod structs;

pub(crate) use expand::expand_bin;
pub(crate) use structs::field_normalizes_aliases;
