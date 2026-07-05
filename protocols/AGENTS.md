# protocols — agent & contributor guide

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it so Claude Code and any
> other AGENTS.md-aware tool read the same instructions. Each protocol crate carries its
> own `AGENTS.md` (+ `CLAUDE.md` symlink) for crate-specific detail; this root file holds
> the workspace-wide rules that apply everywhere.

**What this is.** A Cargo workspace of **from-scratch network-protocol implementations** in
Rust — a typed, compiled, dual-use alternative to Scapy / Impacket. Each protocol is its
own independently-versioned crate, organized by OSI layer (`link/` → `network/` →
`transport/` → `session/` → `application/`). The wire codec throughout is
[`bnb`](https://github.com/RawSocketLabs/bitsandbytes) (published as `bitsandbytes`) — its
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

Workspace-wide policy lives in config so it can't drift; CI enforces all of it:

- **Dependencies** — one version each in `[workspace.dependencies]`; members use
  `dep.workspace = true`. `cargo deny` gates advisories/licenses/duplicate versions
  (`multiple-versions = "deny"`).
- **Lints** — `[workspace.lints]`; members opt in with `[lints] workspace = true`.
  `clippy::all = deny` hard-fails CI; `pedantic` / `missing_docs` / `print_stdout|stderr` are
  advisory `warn`. `module_inception` and `upper_case_acronyms` are relaxed (protocol acronyms
  and file-named-after-module are house style).
- **Formatting** — `rustfmt.toml` (stable options only), `cargo fmt --all --check` in CI.
- **Toolchain / MSRV** — `rust-toolchain.toml` (stable) for day-to-day; MSRV floor
  `rust-version = "1.85"`, edition 2024, checked in a dedicated CI job. **No let-chains**
  (unstable below 1.88).
- **Versioning** — per-crate SemVer from Conventional-Commit type + **scope = crate name**
  (`feat(dns): …` versions `application/dns`), plus a workspace milestone marker. See
  [`VERSIONING.md`](VERSIONING.md).
- **Zero `unsafe`** in codec code — bnb forbids it crate-wide and its macros emit none, so a
  protocol crate can `#![forbid(unsafe_code)]` and still use the codec. Raw-socket I/O (via the
  external `rawsock`) is the only place `unsafe`/FFI is expected, and it's isolated there.

**Tracked lint debt.** A crate not yet clean carries a tracked `#![allow(clippy::all)]` with a
`TODO(hygiene)` note — never add new ones to a clean crate; tighten to the workspace default as
a crate hardens. (The predecessor workspace's debt is not carried here — every crate lands
clean, on bnb, from the start.)

## Cross-cutting tooling is shared, not copied

- **Logging is `tracing`** — libraries emit events; the *application* installs the subscriber
  via `tracing-subscriber`. Libraries never `println!` (enforced by `clippy::print_stdout` /
  `print_stderr = warn`).
- **Test / bench / logging helpers** live in a shared `testutil` dev-dependency (feature-gated:
  `bench::criterion()` one Criterion harness, `logging::init_test()`, `golden::assert_bytes_eq`
  hex-diff byte assertions, and a round-trip macro for `#[bin]` types). New crates get
  cross-cutting tooling there, not by copy-paste. *(Not yet present — added when the second
  crate needs it; the seed crate's tests are self-contained.)*

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
`rfc1035#4.1.4/must.2348d6`. Stay within the crate named in the task.

## Anatomy of a protocol crate (the template)

Every protocol crate follows this shape (its own `AGENTS.md` fills in the specifics):

- **Layout — three layers, dual stance.** `src/wire/` = the typed `#[bin]` codec (the message
  types, `decode`/`to_bytes`); a **raw** path (validation-free constructors + `malformed`
  generators — the dual-use escape hatch); a **high-level** client/server built on the wire
  types. A shared state machine (if any) sits beside them.
- **`#![deny(missing_docs)]`** once the crate is clean; `[lints] workspace = true`.
- **Testing taxonomy** (name each `tests/` file for its role): `smoke` (it's alive), `api`
  (public-surface shape), `contract` (fixed **golden byte vectors** straight from the RFC —
  inline `vec![…]`, the byte-identity anchor), `integration` (composed shapes), `e2e` (real
  transport/loopback), `regression` (where `//~ verifies` lives). Plus inline `#[cfg(test)] mod
  unit` next to each wire type. Golden vectors + round-trip + adversarial (hostile length →
  graceful error, not panic) together.
- **Its own `AGENTS.md`** (title + RFC(s) + refcheck protocol name → this-file blockquote →
  "start here" → architecture → features → entry points → testing → scope notes) and, for
  non-trivial crates, a `DESIGN.md` decision record (numbered sections, non-goals, risks).

## Crate status

Stage legend: **functional** (works + tested) · **dev** (substantial WIP) · **skeleton** (types
only) · **stub** (placeholder) · **planned** (roadmap, no crate yet). `refcheck` = the seeded
corpus name once compliance tracking is wired.

| crate | layer | ver | stage | refcheck |
|---|---|---|---|---|
| `link/ethertype` | link | 0.1.0 | functional | — |
| `link/ethernet` | link | 0.1.0 | functional (frame codec + rawsock injection) | `ethernet` (IEEE 802.3) |
| `transport/tcp` | transport | 0.1.0 | functional (header codec + options + rawsock injection) | `tcp` (RFC 9293) |
| `transport/udp` | transport | 0.1.0 | functional (header codec + rawsock injection) | `udp` (RFC 768) |
| `network/ip` | network | 0.1.0 | functional (header codec + rawsock injection) | `ip` (RFC 791) |
| `network/icmp` | network | 0.1.0 | functional (header codec + rawsock injection) | `icmp` (RFC 792) |
| `application/dns` | application | 0.1.0 | functional (codec + UDP resolver client) | `dns` (RFC 1034/1035) |
| `link/arp` | link | 0.1.0 | functional (packet codec + rawsock injection) | `arp` (RFC 826) |
| `session/socks`; `application/{tftp,smb,nbt,ssh,http,…}` | — | — | planned | — |

The roadmap of protocols-to-come lives here, not as empty member dirs — a crate joins
`[workspace] members` only when it exists.

## Working here

- **One concern per change**, on a branch off `main` → PR → green CI → squash-merge.
- **Conventional Commits**, scope = the crate/protocol name; commitlint enforces the format,
  release-plz derives versions. See [`VERSIONING.md`](VERSIONING.md).
- **No `Co-Authored-By:` trailer in commit messages.** Do not append a `Co-Authored-By:` line
  (the trailer that makes GitHub attribute the commit to a second author) — this applies to
  agent- and human-authored commits alike. Write a plain subject + body.
- **CI gates** (all must pass): fmt, clippy (`clippy::all` denied), build + test, cargo-deny,
  MSRV 1.85.
- The codec is bnb — reach for `#[bin]` / `#[bitfield]` / `#[derive(BitEnum)]` / `#[bitflags]`
  before hand-rolling; a byte run in a custom shape is `parse_with`/`write_with` or a
  `#[bin(codec = …)]` newtype. When bnb can't express something cleanly, that's a **bnb
  finding** — record it (ROADMAP) and, during co-evolution, fix it upstream rather than working
  around it here.
