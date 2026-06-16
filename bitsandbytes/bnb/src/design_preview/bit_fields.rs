//! # Bit fields — the surface unique to `bnb`
//!
//! What binrw cannot do: fields narrower than a byte, and fields that **straddle**
//! byte boundaries in the stream. This is the reason `bnb` exists. (Target design;
//! ` ```rust,ignore `.)
//!
//! ## Any `Bits` type is a field
//!
//! The arbitrary-width integers and the macro-generated types all implement
//! [`Bits`](crate::Bits), so they drop into a `#[bin]` struct with **no `map`
//! glue** and are read at the current *bit* offset:
//!
//! ```rust,ignore
//! #[bin(big)]
//! struct Header {
//!     version: u4,                 // 4 bits
//!     ttl: u5,                     // 5 bits  -> already mid-byte
//!     opcode: OpCode,              // a #[derive(BitEnum)], 5 bits
//!     flags: ControlFlags,         // a #[bitflags] set
//!     window: u14,                 // 14 bits, straddling two bytes
//! }
//! ```
//!
//! No field here is byte-aligned after the first — `bnb` tracks a bit cursor, so
//! this is just three reads. In binrw it would be a single packed integer plus
//! hand-written shift/mask accessors.
//!
//! ## Bit order
//!
//! `#[bin(big, bit_order = msb)]` — `msb` (default) puts the first declared field
//! in the high bits (RFC/ETSI ASCII-art order); `lsb` puts it low. **Byte order**
//! (`big`/`little`) and **bit order** (`msb`/`lsb`) are independent knobs, as in
//! [`#[bitfield]`](crate::bitfield).
//!
//! Bit order is set **per struct**. A message that *mixes* orders puts the
//! differently-ordered run in a nested `#[bitfield]`/group with its own
//! `bits = msb|lsb` (the natural, named unit) — there is no per-field override.
//!
//! ## Sub-byte enums, flags, bitfields — already shipped
//!
//! These compose as bit fields and are the `num_enum`/`modular-bitfield`
//! replacements:
//! - [`#[derive(BitEnum)]`](crate::BitEnum) — enum ⇄ integer with `#[catch_all]`
//!   (unknown values preserved — dual-use).
//! - [`#[bitflags]`](crate::bitflags) — single-bit flag sets.
//! - [`#[bitfield]`](crate::bitfield) — pack a run of sub-byte fields into one
//!   backing integer with named accessors.
//!
//! ## When to use which (the codec steers you)
//!
//! | Your data | Use | Why |
//! |---|---|---|
//! | Fields straddle byte boundaries in the stream | inline bit fields in `#[bin]` | only `bnb` reads at bit offsets |
//! | A run of sub-byte fields that fills one integer | a [`#[bitfield]`](crate::bitfield) field | one packed word, named accessors |
//! | Whole-byte fields | plain fields in `#[bin]` | byte-aligned, simplest |
//!
//! `bnb` enforces this: a message whose fields are **all byte-aligned** gains
//! nothing from the bit cursor, so the codec **rejects it at compile time** and
//! points you at the plain path — overridable with `#[bin(allow_byte_aligned)]`
//! (the spike's `#[bit_stream(allow_byte_aligned)]`). The goal is that "which form
//! do I use?" is answered for you.
//!
//! ## Embedded regions
//!
//! A `#[bin]` (bit) message can be embedded inside an otherwise byte-aligned
//! message; the region must be byte-aligned *overall* (its widths sum to a multiple
//! of 8). The macro asserts this at compile time.
//!
//! ## Reserved / padding bits
//!
//! Unused / MUST-be-zero / future-use bits are **named** with `#[reserved]`, so
//! every bit in a layout is accounted for — no implicit gaps. The default fill is
//! `0`, or a specific value with `#[reserved = expr]` (for "reserved, transmit as
//! 1", fixed pad patterns, etc.):
//!
//! ```rust,ignore
//! #[bin(big)]
//! struct Word {
//!     data_offset: u4,
//!     #[reserved] rsv: u3,               // future-use bits — default 0
//!     #[reserved = 0b1] must_be_one: u1, // reserved, transmit-as-1 by default
//!     flags: ControlFlags,               // 8 bits — 4 + 3 + 1 + 8 = 16, fills exactly
//! }
//! ```
//!
//! `#[reserved]` / `#[reserved = expr]` (mirrors `#[bin(default)]` / `default =
//! expr`) are **builder-optional** with that default, **preserved on decode** (you
//! can observe a peer's non-compliant reserved bits — dual-use), stay **settable**
//! for fuzzing (struct literal / builder override), and **count toward the
//! fill-exactly invariant** — so layouts stay honest with no invisible gaps. For a
//! reserved value that must be *verified on read*, use `magic = <uN>` instead
//! (`reserved` is a lenient default; `magic` is an enforced constant).
//!
//! > **✓ decided:** reserved bits are explicit `#[reserved]` / `#[reserved = expr]`
//! > members, never implicit gaps — honest layouts *and* reserved bits stay
//! > observable (decode) and settable (fuzz). Implicit gaps rejected.
//!
//! Next: [`super::io_model`].
