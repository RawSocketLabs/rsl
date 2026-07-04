//! `#[derive(BitsBuilder)]` — a required-by-default builder.
//!
//! The infix `with_*` setters are infallible and start from an all-zero value, so a
//! field you forget is silently zero. The builder closes that gap: every field is
//! **required** unless opted out, and `build()` returns an error naming the first
//! unset one.
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
