# protocols — agent & contributor guide

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it so Claude Code and any
> other AGENTS.md-aware tool read the same instructions. Each protocol crate carries its
> own `AGENTS.md` (+ `CLAUDE.md` symlink) for crate-specific detail; this file holds the
> protocol-group rules. The workspace root [`AGENTS.md`](../AGENTS.md) (verification, commits,
> versioning, lint and dependency policy) also applies and is canonical for those.

**What this is.** A group of crates in the `rsl` workspace: **from-scratch protocol
implementations** in Rust — a typed, compiled, dual-use alternative to Scapy / Impacket. Each
protocol is its own
independently-versioned crate. Network protocols are organized by OSI layer (`link/` →
`network/` → `transport/` → `session/` → `application/`); local kernel/userspace protocols
live under `system/` rather than being forced into a fictitious OSI layer. `examples/` is not a
layer: it is `protocol-examples`, the unpublished crate of runnable dual-use examples that
compose packets across the stack and inject them through `rawsock`. The wire codec
throughout is [`bnb`](../codecs/bitsandbytes/) (published as `bitsandbytes`) — its
`#[bin]` whole-message macro plus integer-backed bitfields / arbitrary-width ints / enums /
flag-sets. See [`DESIGN.md`](DESIGN.md) for the rationale and [`ROADMAP.md`](ROADMAP.md) for
what's built and what's next.

## Design philosophy: dual-use by default

**This is the load-bearing invariant — read it before touching any parser or constructor.**
These crates are **compliant by default, but deliberately violatable.** The guided/default
path emits and handles RFC-correct traffic; a caller who wants to send non-compliant traffic
(fuzzing, red-teaming, interop testing) must be able to. Therefore:

- **Builder defaults are compliant** (e.g. `version = 5`, `RSV = 0`); the same fields stay
  `pub` so a caller can override them.
- **Parsers accept representable-but-non-compliant values** — model unknown values as
  `Custom(...)` variants rather than hard-erroring. Only reject what is *physically
  unencodable* (e.g. a domain longer than a one-byte length prefix can express), never what
  is merely non-conformant. This is exactly bnb's own doctrine: permissive decode, and encode
  refuses only what could not round-trip.
- **Never enforce a policy requirement inside a parser or raw constructor.** Compliance lives
  on the default path; the raw path is an open escape hatch. Construction-side soundness goes
  through bnb `validate` (which gates a builder, never the parser).
- Intentional non-conformance is documented, not hidden: use the `deviates` or `rejects`
  refcheck annotation verbs (below) with a `reason=`.

## Standards — codified, not by convention

Dependency, lint, formatting, toolchain and versioning policy is workspace-wide and lives in the
root config; the root [`AGENTS.md`](../AGENTS.md) is canonical. What this group adds:

- **Dependencies** — members use `dep.workspace = true`; `cargo deny` gates advisories/licenses,
  and duplicate versions are warned (see root `deny.toml`).
- **Lints** — `[lints] workspace = true`: `clippy::all` is denied; `pedantic` / `missing_docs` /
  `print_stdout|stderr` warn. `module_inception` and `upper_case_acronyms` are relaxed (protocol
  acronyms and file-named-after-module are house style). Never add `#![allow(clippy::all)]`;
  crates land clean.
- **MSRV 1.85, edition 2024** — **no let-chains** (unstable below 1.88).
- **Versioning** — per-crate SemVer, **scope = crate name** (`feat(dns): …` versions
  `application/dns`). See [`VERSIONING.md`](VERSIONING.md).
- **Zero `unsafe`** in codec code — bnb forbids it crate-wide and its macros emit none, so a
  protocol crate can `#![forbid(unsafe_code)]` and still use the codec. The only `unsafe`/FFI is
  in `rawsock` and netlink's documented `setsockopt` boundary (`system/netlink/src/transport.rs`);
  every other protocol crate pins `#![forbid(unsafe_code)]`.

## Cross-cutting tooling is shared, not copied

- **Logging is `tracing`** — libraries emit events; the *application* installs the subscriber
  via `tracing-subscriber`. Libraries never `println!` (enforced by `clippy::print_stdout` /
  `print_stderr = warn`).
- **Tests are self-contained** per crate (golden vectors inline); there is no shared `testutil`
  crate. A shared helper crate is a [`ROADMAP.md`](ROADMAP.md) item with a concrete trigger.

## External utilities & tooling

The codec and raw-I/O crates now live alongside `protocols` as members of the **`rsl` monorepo**
(one Cargo workspace; inter-crate deps are `path` + `version`, so they move in lockstep — no
git-rev pinning). The compliance tooling is still a separate external tool.

- **`bnb`** (published `bitsandbytes`) — **the codec.** A workspace member, consumed as a path
  dep and evolved in lockstep with the protocol ports. `use bnb;`.
- **`rawsock`** — dual-use layered raw-packet I/O (AF_PACKET / raw sockets, checksum/length
  derivation, the `Protocol` injection trait). Also a workspace member; the crates that put
  frames on the wire depend on it behind their `inject` feature (a pure codec needs none of it).
- **`refcheck`** *(external tool, wire up when compliance tracking begins)* — the RFC-compliance
  tracker: it *records* compliance (including deliberate deviation) but never generates or
  constrains runtime behavior — an **observer, not an enforcer**. A requirement in its ledger is
  never a reason to make a parser stricter (see the dual-use invariant). We keep its in-source
  annotation grammar now (below) so the corpus can be wired later without re-annotating.

### refcheck annotation grammar (kept now, tooling wired later)

Annotate a code item with a `//~` line mapping it to a spec requirement, so the external tool
can verify claims don't go stale:

```
//~ <verb> <requirement-id> [key="value" …]
```

Verbs: `implements` (fully), `partial` (`missing="…"`), `deviates` (`reason="…"`), `rejects`
(`reason="…"`), `verifies` (a test), `models` (a whole section — anchors a section id like
`rfc1035#4.1.1`, not a single requirement). Requirement ids look like
`rfc1035#4.1.4/must.2348d6`.

A handshake magic that identifies the protocol (e.g. rtltcp's `RTL0`) may be a codec constant
rather than a stored field; annotate it `//~ rejects … reason=`.

## Anatomy of a protocol crate (the template)

New protocol crates follow this shape (its own `AGENTS.md` fills in the specifics); existing
crates predate parts of it (only socks has `src/*/wire/`):

- **Layout — three layers, dual stance.** `src/wire/` = the typed `#[bin]` codec (the message
  types, `decode`/`to_bytes`); a **raw** path (validation-free constructors + `malformed`
  generators — the dual-use escape hatch); a **high-level** client/server built on the wire
  types. A shared state machine (if any) sits beside them.
- **`#![deny(missing_docs)]`** once the crate is clean; `[lints] workspace = true`.
- **Testing taxonomy** (name each `tests/` file for its role): `api` (public-surface shape),
  `contract` (fixed **golden byte vectors** straight from the RFC — inline `vec![…]`, the
  byte-identity anchor; older crates keep these in `integration`), `integration` (composed
  shapes), `adversarial` (hostile input → graceful error, never a panic), `inject` (the
  `inject` feature's rawsock layer against a `Loopback` sink), `client` (a high-level client
  over real loopback), `regression` (where `//~ verifies` lives). Plus inline `#[cfg(test)] mod
  unit` next to each wire type.
- **Its own `AGENTS.md`** (title + RFC(s) + refcheck protocol name → this-file blockquote →
  status → architecture → injection/features → dual-use → entry points → testing → scope
  notes) and, for
  non-trivial crates, a `DESIGN.md` decision record (numbered sections, non-goals, risks).

## Crate status

Stage legend: **functional** (works + tested) · **dev** (substantial WIP) · **skeleton** (types
only) · **stub** (placeholder). `refcheck` = the seeded
corpus name once compliance tracking is wired.

| crate | layer | stage | refcheck |
|---|---|---|---|
| `link/ethertype` | link | functional | — |
| `link/ethernet` | link | functional (frame codec + rawsock injection) | `ethernet` (IEEE 802.3) |
| `link/arp` | link | functional (packet codec + rawsock injection) | `arp` (RFC 826) |
| `network/ip` | network | functional (header codec + rawsock injection) | `ip` (RFC 791) |
| `network/icmp` | network | functional (header codec + typed message views + rawsock injection) | `icmp` (RFC 792) |
| `transport/tcp` | transport | functional (header codec + options + rawsock injection) | `tcp` (RFC 9293) |
| `transport/udp` | transport | functional (header codec + rawsock injection) | `udp` (RFC 768) |
| `session/socks` | session | dev (SOCKS5 codecs; blocking/Tokio/Mio CONNECT clients, servers, proxies; blocking BIND) | `socks` (RFC 1928/1929) |
| `application/dns` | application | functional (codec + UDP/TCP resolver client) | `dns` (RFC 1034/1035) |
| `application/rtltcp` | application | functional (greeting + command codec, blocking client) | — (no RFC) |
| `system/netlink` | system | functional (route/generic codecs, blocking transport, typed WireGuard API) | `netlink` (Linux UAPI) |
| `examples` (`protocol-examples`) | — | demos, unpublished (forged UDP/ARP over rawsock `network`/`link`) | — |

Current versions are in each crate's `Cargo.toml` (release-plz moves them). Planned protocols
are in [`ROADMAP.md`](ROADMAP.md); a crate joins `members` only when it exists.

## Working here

- **One concern per change.** Verification and publication gates: see the root
  [`AGENTS.md`](../AGENTS.md#verify) and [`docs/CI.md`](../docs/CI.md). Commit rules (Conventional
  Commits, scope = crate name, no `Co-Authored-By:`) are in the root guide; see also
  [`VERSIONING.md`](VERSIONING.md).
- Keep a change within one crate unless it is a cross-crate API move.
- The codec is bnb — reach for `#[bin]` / `#[bitfield]` / `#[derive(BitEnum)]` / `#[bitflags]`
  before hand-rolling; a byte run in a custom shape is `parse_with`/`write_with` or a
  `#[bin(codec = …)]` newtype. When bnb can't express something cleanly, that's a **bnb
  finding** — record it (ROADMAP) and, during co-evolution, fix it upstream rather than working
  around it here.
