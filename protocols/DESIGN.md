# protocols — design

**Status:** scaffold (rev 1) — the conventions, harness, and first seed crate. Protocol crates
land incrementally on `bnb`; see [`ROADMAP.md`](ROADMAP.md).

## §1 What and why

A workspace of typed, from-scratch network-protocol codecs — the compiled, dual-use answer to
Scapy/Impacket. Two goals in tension, both served: **learn the protocols deeply** (the types
mirror the RFC wire format exactly) and **ship fast tooling** (real encoders/decoders, not
inspection scripts).

**Non-goals.** Not a packet-capture framework (that's the transport layer's job, via the external
`rawsock`). Not a compliance *enforcer* (compliance is observed, never imposed — see the dual-use
invariant). Not a serde data model (the wire format is bit-exact; serde has no bit widths or byte
order — same conclusion bnb reached).

## §2 Why per-protocol crates

Each protocol is independently versioned and independently useful: a consumer wanting only DNS
pulls only `dns`, not a monolith. The OSI-layer directory structure (`link/network/transport/
session/application`) makes the dependency direction legible (lower layers don't depend on higher
ones) and mirrors how the protocols actually stack. Cross-cutting concerns (raw I/O, compliance
tracking, test helpers) are *external* crates so they stay independently useful too.

## §3 Why `bnb` as the codec

`bnb` (bitsandbytes) is an owned, bit-aware binary codec purpose-built to collapse the
`binrw` + `bitbybit`/`modular-bitfield` + `num_enum` + `arbitrary-int` stack the predecessor
workspace used into one macro family: `#[bin]` (whole-message), `#[bitfield]`, `#[derive(BitEnum)]`,
`#[bitflags]`, plus `parse_with`/`write_with` and `#[bin(codec = …)]` newtypes for custom shapes.
It shares this workspace's **dual-use doctrine at the codec level** — permissive decode, encode
refuses only the physically unencodable — so the codec and the protocol layer pull in the same
direction. Consuming it from git during the first ports lets protocol needs drive bnb features
before its 1.0 (co-evolution).

## §4 The dual-use contract

The load-bearing invariant, stated once here and enforced everywhere: **compliant defaults, open
escape hatch.** Builders default to RFC-correct values but leave fields `pub`; parsers accept any
representable input (unknowns → `Custom(..)`) and reject only the physically unencodable;
soundness checks live construction-side (`bnb validate`), never in a parser. The full statement
and its rationale are in [`AGENTS.md`](AGENTS.md).

## §5 External-crate boundaries (extraction triggers)

- **`rawsock`** is extracted to its own repo and depended on **when the first protocol needs to
  put frames on the wire** (raw sockets, checksum/length derivation). Until then, protocols are
  pure encoders/decoders and pull no socket code.
- **`refcheck`** is extracted and wired **when compliance tracking begins** for a real protocol
  (starting with DNS's RFC 1034/1035 corpus). The `//~` annotation grammar is kept in-source from
  the start (cheap, codec-agnostic) so no re-annotation is needed when the tool arrives.

## §6 Open risks (tracked, not blocking)

- **bnb git-dep churn** — pin to a rev once co-evolution settles so protocols CI can't break on
  bnb's in-flight `main`.
- **DNS surfaces real bnb gaps** (mutable message-scoped scratch state for name compression;
  overridable stored-length fields) — the co-evolution feature work; tracked in
  [`ROADMAP.md`](ROADMAP.md) and mirrored into bnb's own ROADMAP.
