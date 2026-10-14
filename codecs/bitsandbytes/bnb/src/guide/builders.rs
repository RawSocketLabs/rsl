//! `#[derive(BitsBuilder)]` — a required-by-default builder.
//!
//! The infix `with_*` setters are infallible and start from an all-zero value, so a
//! field you forget is silently zero. The builder closes that gap: every field is
//! **required** unless opted out, and `build()` returns an error naming the first
//! unset one.
//!
//! Builders also normalize enum aliases automatically: `Other(2)` becomes the
//! named variant for discriminant 2. Unknown codes remain unknown. This runs after
//! required/default fields are resolved and before the `validate` hook. Setters
//! only store their arguments; missing-field precedence is unchanged.
//!
//! ```
//! use bnb::{bitfield, u4, BitsBuilder};
//!
//! #[bitfield(u16, bits = msb)]
//! #[derive(BitsBuilder, Clone, Copy)]
//! struct State {
//!     opcode: u4,
//!     flags: u8,
//!     rcode: u4,
//! }
//!
//! let s = State::builder()
//!     .opcode(u4::new(2))
//!     .flags(0)
//!     .rcode(u4::new(3))
//!     .build()
//!     .unwrap();
//! assert_eq!(s.opcode().value(), 2);
//! ```
//!
//! Forget a required field and `build()` tells you which — at run time, by name:
//!
//! ```
//! # use bnb::{bitfield, u4, BitsBuilder, BuilderError};
//! # #[bitfield(u16, bits = msb)] #[derive(BitsBuilder, Clone, Copy, Debug)]
//! # struct State { opcode: u4, flags: u8, rcode: u4 }
//! let err = State::builder().opcode(u4::new(2)).rcode(u4::new(3)).build().unwrap_err();
//! assert_eq!(err, BuilderError::MissingField("flags"));
//! assert_eq!(err.field(), Some("flags"));
//! ```
//!
//! # Opting a field out: `#[builder(default)]`
//!
//! Mark a field optional with `#[builder(default)]` (uses `Default::default()` if
//! unset) or `#[builder(default = expr)]` (uses `expr`):
//!
//! ```
//! use bnb::{bitfield, u4, BitsBuilder};
//!
//! #[bitfield(u16, bits = msb)]
//! #[derive(BitsBuilder, Clone, Copy)]
//! struct State {
//!     opcode: u4,
//!     #[builder(default)]            // 0 if unset
//!     flags: u8,
//!     #[builder(default = u4::new(1))] // a custom default if unset
//!     rcode: u4,
//! }
//!
//! let s = State::builder().opcode(u4::new(2)).build().unwrap(); // flags + rcode defaulted
//! assert_eq!(s.flags(), 0);
//! assert_eq!(s.rcode().value(), 1);
//! ```
//!
//! # On plain structs and the `#[bitfield]` intercept
//!
//! `#[derive(BitsBuilder)]` also works on a plain struct, and `#[bin]` generates the
//! same builder automatically (you don't write the derive there). On a `#[bitfield]`,
//! the attribute collapses the struct to one integer *before* a normal derive could
//! see the fields, so `#[bitfield]` itself intercepts the `BitsBuilder` marker — which
//! is why it must sit **above** the `#[derive(...)]`:
//!
//! ```
//! use bnb::{bitfield, u4, BitsBuilder};
//! #[bitfield(u8, bits = msb)]   // must be above the derive
//! #[derive(BitsBuilder, Clone, Copy)]
//! struct Nibble { hi: u4, lo: u4 }
//! assert_eq!(Nibble::builder().hi(u4::new(0xA)).lo(u4::new(0xB)).build().unwrap().hi().value(), 0xA);
//! ```
//!
//! In a `#[bin]` message the builder is generated for you and is where the
//! [`validate`](super::directives) soundness hook runs — see
//! [`bin_codec`](super::bin_codec).
//!
//! # Alias normalization boundaries
//!
//! [`NormalizeEnumAliases`](crate::NormalizeEnumAliases) traverses ordinary stored
//! fields in `#[bin]` structs/enums and plain `BitsBuilder` structs, recursively
//! through `Vec<T>`, `Option<T>`, and `[T; N]` when their elements support it.
//! Type aliases work. Packed bitfields already retain only bits, so their getters
//! already reconstruct named variants. No field annotation or opt-out is needed.
//!
//! Opaque handwritten types get no new trait bounds. Struct-level mapped/codec
//! newtypes do not receive a generated traversal; a wrapper can explicitly implement
//! the trait. Field-level `map`/`try_map`/`parse_with`/`write_with` directives (either
//! direction), `ignore`, `temp`, and read/write `calc` fields are excluded even if
//! their type implements it. Put custom traversal in a wrapper used as an ordinary
//! field, not behind a field-level callback. Shared references, arbitrary wrappers,
//! and containers other than those listed are not automatically traversed.
//!
//! This is a construction behavior change, including for standalone `BitsBuilder`:
//! Rust variant equality may change, but enum discriminants do not. Ordinary codecs
//! preserve wire bytes; arbitrary user calculations or mappings referring to a
//! normalized sibling can observe its new variant. Audit such callbacks when upgrading.
//! Normalization makes an extra in-place collection pass, even before a validator
//! rejects an oversized collection; it neither allocates nor changes order or length.
//!
//! Direct construction, later mutation, decode, encode, and immutable `validate()`
//! remain unchanged. Validators must still defend against aliases introduced outside
//! builders. Alias normalization does not repair reserved fields or derived lengths;
//! canonical encoding remains a separate operation.
//!
//! For an explicit operation, use `value.normalize_enum_aliases()` to mutate in
//! place or `value.into_normalized_enum_aliases()` to consume and return the value.
//! Import [`NormalizeEnumAliases`](crate::NormalizeEnumAliases) to call either.
//! The consuming form adds no allocation and needs neither `Clone` nor `Copy`;
//! use `.clone().into_normalized_enum_aliases()` explicitly to retain an owned
//! original. For a `Copy` enum, an immutable validator can compare
//! `method.into_normalized_enum_aliases() == Method::Named` without changing the
//! stored variant. See the trait method's executable ownership examples.
//!
//! Dispatch is generated at concrete field types, preserving the existing macro
//! generic-parameter limitations. A handwritten generic wrapper must use an explicit
//! `T: NormalizeEnumAliases` bound to recurse; the internal fallback cannot discover
//! implementations later at monomorphization.
