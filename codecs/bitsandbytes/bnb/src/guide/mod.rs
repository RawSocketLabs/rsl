//! The `bnb` guide — worked, runnable walkthroughs.
//!
//! Every page here is a normal rustdoc module whose examples are compiled and run as
//! doctests, so nothing in the guide can drift from the code. Read the pages in this
//! order (the same reading order given in the [crate root](crate)), or jump to a topic:
//!
//! 1. [`quick_start`] — a five-minute tour of every macro.
//! 2. [`numbers`] — `u1`..`u127` and the [`Bits`](crate::Bits) trait.
//! 3. [`bitfields`] — `#[bitfield]`: bit/byte order, widths, ranges, nesting.
//! 4. [`enums`] — `#[derive(BitEnum)]`: catch-all, closed, `num_enum` parity.
//! 5. [`flags`] — `#[bitflags]`: single-bit flag sets with set algebra.
//! 6. [`builders`] — `#[derive(BitsBuilder)]`: the required-by-default builder.
//! 7. [`bin_codec`] — `#[bin]`: a whole protocol header, end to end.
//! 8. [`directives`] — the field-directive reference, one example each.
//! 9. [`mapping`] — `#[bin(map/bw_map = …)]`: a whole struct mapped to/from a wire type.
//! 10. [`dispatch`] — `#[bin]` on an enum: tagged-union dispatch by wire `magic` or off-wire `tag`.
//! 11. [`io`] — the `Source`/`Sink` I/O ladder.
//! 12. [`errors`] — position-aware errors and the streaming `Incomplete` signal.
//! 13. [`dual_use`] — compliant by default, deliberately violatable.
//! 14. [`composition`] — how the pieces nest and size each other.
//! 15. [`migrating`] — coming from `binrw` / `modular-bitfield` / `num_enum`.
//!
//! # How the crate fits together
//!
//! One trait, [`Bits`](crate::Bits), is the keystone: a value that occupies a fixed
//! number of bits. The arbitrary-width integers (`u1`..`u127`), `bool`, the primitive
//! integers, and every type the macros generate all implement it, so they **compose
//! as fields** without glue:
//!
//! - `#[bitfield]` packs several `Bits` values into one backing integer.
//! - `#[derive(BitEnum)]` makes an enum a `Bits` value (an integer discriminant).
//! - `#[bitflags]` makes a flag set a `Bits` value.
//! - `#[bin]` reads/writes a whole message of `Bits` fields at arbitrary bit offsets,
//!   and a `#[bitfield]`/`BitEnum`/`#[bitflags]` drops in as one field.
//! - `#[derive(BitsBuilder)]` adds a required-by-default builder to any of the above.
//!
//! Because the unit of composition is "a value of N bits", a 5-bit enum nests in a
//! 16-bit bitfield which nests in a byte-aligned message — all checked at compile
//! time by const-evaluated width arithmetic, never by the proc-macro guessing widths.

pub mod bin_codec;
pub mod bitfields;
pub mod builders;
pub mod composition;
pub mod directives;
pub mod dispatch;
pub mod dual_use;
pub mod enums;
pub mod errors;
pub mod flags;
pub mod io;
pub mod mapping;
pub mod migrating;
pub mod numbers;
pub mod quick_start;
