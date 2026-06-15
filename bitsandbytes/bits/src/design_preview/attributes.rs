//! # Attribute reference (`br` / `bw` / `brw`)
//!
//! The directive language, inherited from binrw and extended to be bit-aware.
//! `#[br(...)]` applies on **read**, `#[bw(...)]` on **write**, `#[brw(...)]` on
//! both. Multiple directives stack. (Target design; ` ```rust,ignore `.)
//!
//! Priority for *building* these (by real workspace usage, `DESIGN.md` §9.1):
//! `magic ≫ pre_assert ≈ endianness > map > count > ctx > ignore >
//! calc > parse_with > if > restore_position > temp`. See `ROADMAP.md` Phase 2.
//!
//! ## Summary
//!
//! | Directive | Side | Purpose | Bit-aware |
//! |---|---|---|---|
//! | [`magic`](#magic) | brw | constant marker read-and-verified / written | ✓ (any width) |
//! | [`calc`](#calc) | bw | compute the value on write | ✓ |
//! | [`temp`](#temp) | br | field read but not stored | ✓ |
//! | [`ignore`](#ignore) | br | skip on read (use `default`) | ✓ |
//! | [`map` / `try_map`](#map--try_map) | brw | transform value ⇄ stored | ✓ |
//! | [`count`](#count) | br | element count for a `Vec` | n/a |
//! | [`ctx`](#ctx) | brw | parameterized parse — feed context from the parent | ✓ |
//! | [`if`](#if) | brw | conditional field (`Option`) | ✓ |
//! | [`pad_* / align_* / seek`](#positioning) | brw | positioning & padding | ✓ (bit or byte) |
//! | [`restore_position`](#restore_position) | brw | read/write then rewind | ✓ |
//! | [`assert` / `pre_assert`](#assertions) | brw | checks (dual-use: not parser rejects) | ✓ |
//! | [`parse_with` / `write_with`](#parse_with--write_with) | brw | full raw-codec escape hatch | ✓ |
//! | [`default`](#default-builder) | builder | opt a field out of required-by-default | — |
//!
//! ---
//!
//! ## `magic`
//! A constant that must be present on read and is emitted on write. Unlike binrw,
//! the width can be sub-byte.
//! ```rust,ignore
//! #[brw(magic = 0x7Eu8)]           // a one-byte delimiter
//! #[brw(magic = 0b110u3)]          // a 3-bit tag (bit-aware)
//! ```
//! On read, a mismatch is a `bnb::Error::BadMagic { expected, found, at }`.
//!
//! ## `calc`
//! Compute the field on **write** from other fields; pair with `temp` so it is not
//! stored. Lengths, counts, checksums.
//! ```rust,ignore
//! #[bw(calc = self.items.len() as u16)]
//! #[br(temp)]
//! count: u16,
//! #[br(count = count)]
//! items: Vec<Item>,
//! ```
//!
//! ## `temp`
//! The field is parsed into a local (usable by later directives like `count`) but
//! **not stored** in the struct. The matched read/write pair is generated together
//! so the two directions can't drift.
//!
//! ## `ignore`
//! On read, don't consume input; initialize from `default` (or `Default`). On
//! write, don't emit. (For a value present on the wire but absent from the builder,
//! see `builder_only` in [`super::escape_hatches`].)
//!
//! ## `map` / `try_map`
//! Transform between the wire representation and the field type. `try_map` is the
//! fallible form.
//! ```rust,ignore
//! #[br(map = |raw: u32| Ipv4Addr::from(raw))]
//! #[bw(map = |ip: &Ipv4Addr| u32::from(*ip))]
//! addr: Ipv4Addr,
//!
//! #[br(try_map = SyncPattern::try_from)]   // errors flow to bnb::Error
//! sync: SyncPattern,
//! ```
//! Often unnecessary in `bnb`: a `#[derive(BitEnum)]`/`#[bitfield]` field needs no
//! `map` glue (the headline win over a byte codec).
//!
//! ## `count`
//! Element count for a `Vec<T>` (or bit count for a bit slice).
//! ```rust,ignore
//! #[br(count = header.n)]
//! records: Vec<Record>,
//! ```
//!
//! ## `ctx`
//!
//! **Parameterized parsing — context fed from outside.** For when a type needs a
//! runtime value to parse itself that comes from the
//! *parent*, not its own bytes — versioned bodies, tag-dispatched TLV values,
//! a shared string table. (binrw calls these `import`/`args`; `ctx` is the clearer
//! spelling.) Two halves:
//!
//! - **Declare** what a type needs, on the type: `#[bin(ctx(tag: u8, version: u8))]`
//!   — those names are then in scope for the type's fields/directives.
//! - **Pass** it, on a field: `#[br(ctx { tag, version })]` — feeds the field's
//!   parser. (`#[bw(ctx { … })]` / `#[brw(ctx { … })]` for the write/both sides.)
//!
//! ```rust,ignore
//! // A TLV whose Value is parsed according to the Tag read just before it:
//! #[bin(big)]
//! struct Tlv {
//!     tag: u8,
//!     #[br(ctx { tag })]
//!     value: Value,          // generated: Value::decode_with(src, ValueCtx { tag })
//! }
//! #[bin(big, ctx(tag: u8))]  // Value declares it needs `tag`
//! enum Value { /* variants selected by tag */ }
//!
//! // A SEQUENCE-OF whose elements each need the parent's version:
//! #[bin(big, ctx(version: u8))]
//! struct Container {
//!     n: u16,
//!     #[br(count = n, ctx { version })]
//!     items: Vec<Item>,      // generated loop: Item::decode_with(src, ItemCtx { version })
//! }
//! ```
//!
//! **How it works (Layer 1, `ROADMAP.md`):** `ctx` lowers to generated **inherent**
//! `Type::decode_with(src, ctx)` methods + a small `Ctx` struct; the macro emits a
//! *concrete* `decode_with` call at every parameterized field, enum arm, and
//! count-loop. So it covers structs/enums/`Vec`s and **arbitrary nesting**
//! (ASN.1/TLV) with **no `Args` associated type on the core `Decode`/`Encode`
//! trait** — the everyday no-context type stays on plain `decode`. Borrowed context
//! (`ctx(table: &StringTable)`) is fine; the lifetime is local to the call.
//!
//! A **`DecodeWith<A>` companion trait** (for hand-written generic combinators /
//! trait-object parsing) is a deferred, **additive** Layer 2 — `Type::decode_with`
//! call sites are unchanged when it lands.
//!
//! ## `if`
//! A conditional field; absent ⇒ `None` (or a supplied default).
//! ```rust,ignore
//! #[br(if(self.flags.has_options))]
//! options: Option<Options>,
//! ```
//!
//! ## Positioning
//! `pad_before` / `pad_after` (fixed gap), `align_before` / `align_after` (round to
//! a boundary), `pad_size_to`, `seek = expr` (absolute). Amounts are **always
//! typed** — `N.bits()` or `N.bytes()` (helpers in `bnb::prelude`) — so the unit is
//! never ambiguous, the values work with field variables, and odd layouts compose:
//! ```rust,ignore
//! #[brw(align_before = 1.bytes())]            // pad to the next byte boundary
//! #[brw(pad_after    = 2.bytes())]            // two zero bytes
//! #[brw(pad_after    = 5.bits())]             // five reserved bits
//! #[br(seek          = hdr.offset.bytes())]   // absolute byte offset from a field
//! #[brw(pad_after    = 1.bytes() + 4.bits())] // 12 bits — odd PHY layout
//! ```
//! Consistent with the reader's unit-explicit `seek_to_bit`/`align_to_byte`. On the
//! in-memory cursor these need no `Seek` trait (see [`super::io_model`]).
//!
//! ## `restore_position`
//! Read or write the field, then return the cursor to where it was (peek / patch).
//! Free on the in-memory cursor.
//!
//! ## Assertions
//! `assert(cond, "msg")` checks during construction/`build()`; `pre_assert(cond)`
//! checks before parsing a field.
//!
//! <div class="warning">
//!
//! **Dual-use rule:** assertions are **construction-time and opt-in**, never a hard
//! parser reject. `Decode` stays permissive — it never refuses representable
//! input (fuzzing/red-team). Validation belongs in `build()`/`validate`, not in the
//! parser. See [`super::escape_hatches`].
//!
//! </div>
//!
//! ## `parse_with` / `write_with`
//! The **escape hatch**: hand a field to a custom function and bypass the generated
//! codec entirely. This is how *anything* `bnb` doesn't model natively is still
//! expressible (and how the spike bridges to binrw today).
//! ```rust,ignore
//! #[br(parse_with = my_reader)]      // fn(&mut R, ..) -> bnb::Result<T>
//! #[bw(write_with = my_writer)]      // fn(&T, &mut W, ..) -> bnb::Result<()>
//! exotic: Exotic,
//! ```
//!
//! ## `default` (builder)
//! Opts a field out of required-by-default in the generated builder:
//! `#[bin(default)]` or `#[bin(default = expr)]`. (Codec behavior unchanged.)
//!
//! > **✓ decided:** parameterized parsing is `ctx` (declare `ctx(...)`, pass
//! > `ctx { … }`), built via generated inherent `decode_with` methods (Layer 1 —
//! > covers declarative ASN.1/TLV with no `Args` type on the core trait). The
//! > generic-combinator `DecodeWith<A>` companion trait is a deferred, additive
//! > Layer 2 (`ROADMAP.md`), so no call-site churn when it lands.
//!
//! > **✓ decided:** positioning amounts are **always typed** — `N.bits()` /
//! > `N.bytes()` (helpers in `bnb::prelude`, composable with `+`). No bare-integer
//! > unit ambiguity; consistent with `seek_to_bit`/`align_to_byte`.
