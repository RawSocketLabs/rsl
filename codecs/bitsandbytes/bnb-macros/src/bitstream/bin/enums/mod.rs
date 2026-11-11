//! `#[bin]` on an enum, a dispatched tagged union: the magic constants, the dispatch
//! model, the per-variant field codec, and the front-end that emits it all.

mod dispatch;
mod expand;
mod magic;
mod variant;

pub(super) use expand::bin_enum;
