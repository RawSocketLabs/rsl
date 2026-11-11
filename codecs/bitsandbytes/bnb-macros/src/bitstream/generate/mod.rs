//! The shared code generators behind the bare derives and `#[bin]`: per-field reads and
//! writes, the message-level `gen_decode`/`gen_encode`, and what they emit around them.

mod decode;
mod encode;
mod entry_points;
mod layout;
mod options;
mod positioning;
mod width;

pub(super) use decode::{field_read_stmt, gen_decode};
pub(super) use encode::{WriteSite, field_write_stmt, gen_encode};
pub(super) use entry_points::{DECODE_ALL_DOC, DECODE_ITER_DOC, DecodeEntryPoints, PEEK_DOC};
pub(super) use layout::{decode_layout_impl, layout_token};
pub(super) use options::BitStreamAttrs;
