# `rsl` — the RawSocket Labs owned-library facade

A single, feature-gated re-export of the public libraries RSL **owns** — the codec, the
protocol implementations, raw-socket I/O, RF parsing, SDR bindings. Depend on `rsl` to consume
RSL's own products through one crate with unified, pinned versions, instead of git-depending on
each library by hand.

```toml
[dependencies]
rsl = { git = "https://github.com/RawSocketLabs/rsl", features = ["net"] }
```

```rust
use rsl::codec;        // bnb (bitsandbytes)
use rsl::proto::dns;   // owned protocol crate
use rsl::rawsock;      // L2/L3/L4 raw-packet I/O
```

## Deps vs. libraries

The RSL stack is split by concern — **what we own** vs. **what we rent**:

- **`rsl`** (this crate) — the libraries we own, under semantic paths (`rsl::codec`,
  `rsl::proto`, `rsl::rawsock`, `rsl::rf`, `rsl::usdr`).
- **[`rsl-deps`](https://github.com/RawSocketLabs/rsl-deps)** — the blessed third-party crates
  (`rsl_deps::tokio`, `rsl_deps::serde`, …). The version-pinned "rented" half.
- **`rsl-private`** — the private owned libraries (`sdr`, `dsdcc`), layered on `rsl`.

The two are **orthogonal**: a crate that needs both RSL libraries and blessed externals depends
on both `rsl` and `rsl-deps`. That separation is the answer to "what do we own, and what might we
replace?" — and it makes the graduation below a clean, one-move change.

## Namespace map

Owned crates, sourced by `git` (pinned rev) from their public `RawSocketLabs` repos:

| Path | Backed by | Feature |
|------|-----------|---------|
| `rsl::codec` | `bitsandbytes` (`bnb`) — bit-aware binary codec | `codec` |
| `rsl::proto::{ethertype,tcp,udp,dns}` | the `protocols` workspace | `proto` (or `proto-<name>`) |
| `rsl::rawsock` | `rawsock` — L2/L3/L4 raw-packet I/O | `rawsock` |
| `rsl::rf` | `rfus` — RF/sample-rate/scan-target parsing | `rf` |
| `rsl::usdr` | `usdr` — USDR SDR bindings (FFI, needs C++ toolchain) | `usdr` |

Bundles: `net` (`codec`+`proto`+`rawsock`), `radio` (`rf`), `full` (`net`+`radio`). The FFI
`usdr` is excluded from bundles — add it explicitly.

**Not here** (by design): the private `libsdr` / `rust-dsdcc` — they live in the private
`rsl-private` overlay. Public/private boundary = repo boundary.

## The graduation path (rented → owned)

When RSL writes an owned crate to supersede a blessed external, the pin **moves out of
[`rsl-deps`] and the owned crate is added here**, under a semantic path. Consumers switch from
`rsl_deps::foo` to `rsl::foo`. Keeping the two crates separate is what makes that replacement
decision legible: `rsl-deps` is the standing shortlist of "things we rent"; `rsl` is what we own.

[`rsl-deps`]: https://github.com/RawSocketLabs/rsl-deps

## Sourcing & caveats

- **Owned crates are `git` deps pinned to an exact `rev`** so a push to a sibling's `main` can't
  silently change or break `rsl` or its consumers. Bump a pin deliberately (`git ls-remote` →
  update the rev). The public `rsl::…` paths don't change when a pin moves.
- **One codec.** `bnb` and the `protocols` crates are pinned to the **same** `bitsandbytes` rev,
  so enabling both `codec` and `proto` compiles exactly one codec whose types unify. (The
  `protocols` repo rev-pins bnb for this reason — a floating pin there would reintroduce a
  duplicate.)
- **`publish = false`.** Git dependencies can't appear in a crates.io release, so `rsl` is
  consumed directly as a git dependency.
- **FFI feature `usdr` needs a C++ toolchain** and is off by default.

## Verify

```sh
cargo build --features full   # codec, proto, rawsock, rf (excludes the FFI usdr)
```
