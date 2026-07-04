# link/ethertype

The 16-bit **EtherType** — the Ethernet II field naming the encapsulated protocol (IEEE 802.3 /
the IANA ethertype registry). refcheck protocol name: *(none yet — leaf value type)*.

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The workspace root
> [`AGENTS.md`](../../AGENTS.md) (dual-use philosophy, standards, the codec) also applies.

## What it is

The workspace's **harness proof + smallest template**: one `bnb` `#[derive(BitEnum)]` at a
byte-aligned `u16` (network order), with a `#[catch_all] Custom(u16)` making it dual-use. It
exists to prove the toolchain end-to-end (the bnb git-dependency resolves, CI is green, the
per-crate lint/test shape holds) and to model the pattern richer protocol crates follow.

## Architecture

Trivial by design — a single value type, no wire *message* codec, no client/server. The three
named things worth knowing:

- `EtherType` (`src/ethertype.rs`) — the enum. Named registry values + `Custom(u16)` catch-all.
- The derive gives, for free: `From<EtherType> for u16`, an infallible `From<u16> for EtherType`
  (via the catch-all), and the `Bits`/codec impls so it reads/writes through `bnb` `Source`/`Sink`
  and nests in a `#[bin]` frame or a `#[bitfield]`.
- Dual-use: unknown ethertypes are `Custom`, never an error — the load-bearing house rule.

## Entry points

- [`EtherType`] — the only public item (re-exported from the crate root).

## Testing

Inline `#[cfg(test)] mod unit` in `src/ethertype.rs`: golden network-order bytes
(`IPv4 → 08 00`), round-trip of every named value, the dual-use property (unknown value
preserved + round-trips), the integer conversions, and the `Default`. Plus the doctest on
`EtherType`. Run: `cargo test -p ethertype`.

## Scope notes

- **Never reject an unknown ethertype** — it is `Custom(raw)`. Only the *physically unencodable*
  (nothing, here — every `u16` is representable) would be refused. This is the dual-use invariant
  in its simplest form.
- No `rawsock` dependency: this crate only names a value; putting frames on the wire is a
  higher-layer concern.
