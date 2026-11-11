//! `#[bin]` on a struct: the named-field message, the wire-mapped struct, and the codec
//! newtype.

mod mapped;
mod named;
mod newtype;

pub(super) use named::bin_struct;
pub(crate) use named::field_normalizes_aliases;
