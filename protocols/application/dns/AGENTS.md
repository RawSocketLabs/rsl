# application/dns

DNS message codec (RFC 1034/1035) on `bnb`. refcheck protocol name: **`dns`**.

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The workspace root
> [`AGENTS.md`](../../AGENTS.md) (dual-use philosophy, standards, the codec) also applies.

## Status — Increment 1 (the pure codec)

**Decode** (following name-compression pointers inline) and **uncompressed encode**. This
is the flagship `bnb` port. Deferred to later increments: **encode-side name compression**
(needs a `bnb` feature — mutable message-scoped scratch state) and a **client/network
layer** (needs the external `rawsock`). Both are tracked in the workspace ROADMAP.

## Architecture

A pure wire codec (no I/O yet). One module per wire concept, all `#[bin]`-based:

- `header.rs` — the 12-byte header. `State` is a flat `#[bitfield(u16)]` (QR/OPCODE/AA/TC/
  RD/RA/Z/RCODE) — `bnb` bitfields need byte-width backing, so the reference crate's
  sub-byte `OpCode`/`Flags` groupings are **flattened** into leaf fields (more RFC-faithful).
  `Op`/`RCode` are `#[derive(BitEnum)]` with `Other` catch-alls.
- `name.rs` — `Name`, a **`#[bin(codec = …)]` newtype** whose label codec follows
  compression pointers inline on decode (via `seek`, bounded against loops) and writes
  uncompressed on encode. Used as a plain field everywhere via `#[brw(variable)]`.
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

- Inline `#[cfg(test)] mod unit` in `header.rs` (bit packing) and `name.rs` (compression,
  loop bound, round-trip).
- `tests/contract.rs` — **golden wire vectors** carried from the reference implementation
  (the decode-fidelity anchor): the uncompressed + compressed `example.com` packets, and an
  unknown-type raw-RDATA case. Uncompressed messages round-trip byte-identically; a
  compressed message decodes with names resolved inline (re-encode is uncompressed).
- `tests/adversarial.rs` — pointer loops/cycles, truncation, oversized RDLENGTH,
  out-of-range pointers, and "decode of arbitrary bytes never panics".

Run: `cargo test -p dns`.

## Scope notes

- `missing_docs` stays at the workspace `warn` (not `deny`): `bnb`'s `#[bin(ctx(...))]`
  generates a `…Ctx` struct with undocumented fields — a `bnb` finding tracked in the
  ROADMAP. Every hand-written item is documented.
- Structured RDATA is the **common set** only; DNSSEC/exotic types are `Custom` (raw bytes)
  by design — add structured variants as demand warrants. TXT/OPT keep raw bytes with view
  helpers rather than a full character-string type (a later refinement).
- SRV/CAA targets are kept as raw bytes (they can embed compression the enclosing record
  can't cleanly resolve at the field level yet).
