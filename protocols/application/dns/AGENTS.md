# application/dns

DNS message codec (RFC 1034/1035) on `bnb`. refcheck protocol name: **`dns`**.

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The workspace root
> [`AGENTS.md`](../../AGENTS.md) (dual-use philosophy, standards, the codec) also applies.

## Status — Increment 2 (compressing codec)

**Decode** (following name-compression pointers inline) plus **both encode forms**:
`to_bytes` (uncompressed) and `to_compressed_bytes` (RFC 1035 §4.1.4 suffix compression).
This is the flagship `bnb` port. Compression rides on the `bnb` `Sink::scratch` feature
(a message-scoped [`CompressionDict`] in the sink's scratch) that this port drove upstream.
Deferred: a **client/network layer** (needs the external `rawsock`), tracked in the ROADMAP.

## Architecture

A pure wire codec (no I/O yet). One module per wire concept, all `#[bin]`-based:

- `header.rs` — the 12-byte header. `State` is a flat `#[bitfield(u16)]` (QR/OPCODE/AA/TC/
  RD/RA/Z/RCODE) — `bnb` bitfields need byte-width backing, so the reference crate's
  sub-byte `OpCode`/`Flags` groupings are **flattened** into leaf fields (more RFC-faithful).
  `Op`/`RCode` are `#[derive(BitEnum)]` with `Other` catch-alls.
- `name.rs` — `Name`, a **`#[bin(codec = …)]` newtype** whose label codec follows
  compression pointers inline on decode (via `seek`, bounded against loops). On encode it
  emits a suffix pointer when the sink carries a `CompressionDict` scratch, else writes
  uncompressed — so the same codec serves both `to_bytes` and `to_compressed_bytes`. Used
  as a plain field everywhere via `#[brw(variable)]`.
- `question.rs` — `Question` + `QType`/`QClass` (BitEnum + catch-all).
- `record.rs` — `Record` + the `RType`/`RClass` registries (BitEnum + catch-all).
- `rdata.rs` — `RData`, a `tag`-dispatched (by `rtype`, with `rdlength` as aux ctx) union:
  structured variants for the common types (A/AAAA/NS/CNAME/PTR/SOA/MX/TXT/SRV/CAA/OPT),
  and a `Custom { rtype, bytes }` **catch-all that preserves any other type's raw RDATA**.
- `message.rs` — `Message`, the top-level `#[bin]`; each section `Vec` sized by the
  header's count.

## The dual-use rule here

Never reject or corrupt representable input. Unknown record types / classes / opcodes are
`Custom`/`Other` (value preserved); unknown RDATA is kept as raw bytes, **not** misparsed
(the reference crate's 36 stubbed `Name`-typed records were a bug — fixed by the `Custom`
fallback). Section counts are plain stored `u16`s: `Message::assemble` derives them from
the sections, but a caller may set a header count that *disagrees* with its section on
purpose (forging a malformed frame). The parser never enforces policy.

## Entry points

`Message` (`decode_exact` / `to_bytes` / `assemble` / `query`), `Header`, `Name`,
`Question`, `Record`, `RData`, and the enums `Op`/`RCode`/`RType`/`RClass`/`QType`/`QClass`.

## Testing

Four layers, each runnable on its own (`cargo test -p dns <layer>`):

- **`unit`** — inline `mod unit` in each `src/*.rs`: pure type logic, no wire codec (State bit
  packing, enum ⇄ int round-trips, `Name::from_str`/`byte_len`, `RData::txt_strings`).
- **`component`** — inline `mod component` in each `src/*.rs`: a *single* wire type through the
  bnb `Source`/`Sink` seam (`Header`/`Name`/`Record`/`Question` round-trips, `Name` compression
  following, each `RData` variant via `RDataCtx`).
- **`integration`** — `tests/integration.rs`: whole-`Message` **golden wire vectors** carried from
  the reference implementation (the decode-fidelity anchor) — the uncompressed + compressed
  `example.com` packets and an unknown-type raw-RDATA case. Uncompressed round-trips
  byte-identically; a compressed message decodes with names resolved inline (re-encode is
  uncompressed).
- **`adversarial`** — `tests/adversarial.rs`: pointer loops/cycles, truncation, oversized
  RDLENGTH, out-of-range pointers, and "decode of arbitrary bytes never panics".

Plus runnable **examples** (`cargo run -p dns --example <name>`): `decode_response` (walk a real
response; unknown types preserved), `build_query` (construct + encode a query), `compress_message`
(`to_compressed_bytes` vs `to_bytes`, and both round-trip), `dual_use_forge` (emit a header whose
count deliberately disagrees with its section). `testutil` is deferred — the golden vectors are
inline until a second crate would share the helpers.

Run everything: `cargo test -p dns`.

## Scope notes

- `#![deny(missing_docs)]` is on (the bnb `#[bin(ctx(...))]` ctx-field-docs finding is fixed
  upstream).
- Structured RDATA is the **common set** only; DNSSEC/exotic types are `Custom` (raw bytes)
  by design — add structured variants as demand warrants. TXT/OPT keep raw bytes with view
  helpers rather than a full character-string type (a later refinement).
- SRV/CAA targets are kept as raw bytes (they can embed compression the enclosing record
  can't cleanly resolve at the field level yet).
