# rsl — agent & contributor guide (workspace root)

> `CLAUDE.md` is a symlink to this file. Each crate carries its own `AGENTS.md` with
> crate-specific detail; this root file holds the workspace-wide rules.

**What this is.** The `rsl` monorepo: one Cargo workspace holding the public RawSocket Labs
stack — the codec (`bitsandbytes`), raw-packet I/O (`rawsock`), cryptography (`crypto`),
ASN.1/X.509/PKI (`pki/*`), compression (`compression`), error correction (`error-correction`),
RF parsing (`rfus`), the protocol crates (`protocols/*`), the facades (`rsl`, `deps`), and
(excluded, FFI) `usdr-ffi` and `rust-dsdcc`. Crates are **published independently** but developed
**in lockstep** — inter-crate deps are `path` + `version`, so there's one `Cargo.lock` and no
git-rev pinning.

## Layout

One directory per domain group; each crate's own `AGENTS.md` holds its detail.

| Path | Crate(s) | Notes |
|------|----------|-------|
| `codecs/` | `bitsandbytes` + `bitsandbytes-macros` (`bitsandbytes/{bnb,bnb-macros}`), `compression`, `error-correction` | the bit-aware codec (`#![forbid(unsafe_code)]`) and accuracy-first compression and redundancy coding (`no_std + alloc`, zero `unsafe`) |
| `crypto/` | `crypto` (`core/`), `crypto-legacy` (`legacy/`) | accuracy-first primitives (`no_std + alloc`, zero `unsafe`); opt-in historical/broken cryptography, never a default negotiation source; `fuzz/` is the group's detached fuzz workspace |
| `pki/` | `asn1`, `x509`, `pki` (`asn1/`, `x509/`, `validation/`) | strict DER transport, certificate syntax and path validation; `no_std + alloc`, zero `unsafe` |
| `transports/` | `rawsock`, `usb` | L2/L3/L4 raw I/O via `rustix` (`#![deny(unsafe_code)]`, one confined `unsafe` fn); owned USB host access over Linux usbfs (unpublished) |
| `rf/` | `rfus` | RF/sample-rate parsing |
| `devices/` | drivers, simulators, oracles, shared chips, `hal` | pure-Rust device drivers, unpublished; see `devices/AGENTS.md` |
| `protocols/<layer>/<proto>` | `ethertype`, `ethernet`, `arp`, `tcp`, `udp`, `ip`, `icmp`, `socks`, `dns`, `netlink` | dual-use protocol implementations on `bnb`; `system/netlink` covers the non-OSI Linux kernel/userspace protocol |
| `rsl/` | `rsl` | owned-library facade (re-exports the above) |
| `deps/` | `deps` | blessed external-dependency stack |
| `bindings/` | `usdr-ffi`, `rust-dsdcc`, `bladerf`, `bladerf-sys` | **excluded** FFI workspaces (need native toolchains/libraries) |
| `tools/rust-skills/` | `xtask` (private) | independently versioned Rust engineering skills and adapters; not auto-activated |

## Standards (workspace-wide, in root config)

- **Deps** — inter-crate deps are `path` + `version` in `[workspace.dependencies]`; external
  versions are pinned there (and, for the blessed stack, in `deps`). `cargo deny` gates
  advisories/licenses; duplicate versions are warned (the facade pulls big trees).
- **Lints** — `[workspace.lints]`; members opt in with `[lints] workspace = true`. The shared
  unsafe policy is `unsafe_op_in_unsafe_fn = deny`; crates that guarantee zero unsafe pin
  `#![forbid(unsafe_code)]` at crate level (bnb); rawsock is `deny` with one confined `unsafe` fn.
- **Formatting / toolchain / MSRV** — root `rustfmt.toml`, `rust-toolchain.toml`, MSRV 1.85.
- **Versioning** — per-crate SemVer via `release-plz` (scope = crate name, e.g. `feat(dns): …`);
  `publish = false` until the registry decision flips on.
- **Commits** — Conventional Commits (commitlint). **No `Co-Authored-By:` trailer.**

## FFI members (`usdr-ffi`, `rust-dsdcc`, `bladerf`, `bladerf-sys`)

Excluded from the default workspace build/CI (they need a C++ toolchain + system libs). Build
them explicitly: `cargo build --manifest-path bindings/usdr/Cargo.toml`. `rust-dsdcc` links the external
DSDcc library — its license governs distributed linked binaries.

`bindings/bladerf/` retains its own workspace with the `bladerf-sys` member in
`sys/`. It requires `libbladeRF` development headers/library and `libclang`.
CI compiles all targets and runs only hardware-free configuration tests; the
remaining device tests are opt-in and serial (`make -C bindings/bladerf test`).

## Verify

### Routine local iteration

Keep checks proportional to the current edit, not the cumulative branch diff. Discussion,
inspection, and status questions do not trigger verification runs.

- Prose/guidance-only edits: review the diff and run `git diff --check`. Build docs or run
  doctests only when changed Rust examples, public API paths, or links need validation.
- Rust edits: run `cargo fmt --all --check`, focused tests for the affected package/behavior,
  and `cargo clippy -p <package> --all-targets -- -D warnings`.
  Enable the features needed by the changed code; do not test unrelated packages.
- Shared code, module moves, or feature wiring: add the relevant compile/test configurations
  (typically default and all-features), not the exhaustive feature matrix. Security-sensitive
  changes still require targeted regressions and independent review.
- Do not automatically run workspace-wide tests/Clippy, exhaustive feature or MSRV matrices,
  release-profile suites, fuzz campaigns, mutation campaigns, benchmarks, or container CI
  during ordinary editing. Reserve full local qualification for release preparation or an
  explicit request for broader verification. Crate full-tier checklists describe that scope,
  not a requirement to repeat it after each local edit.
- Reuse results while their relevant inputs are unchanged. Report exactly what ran; a focused
  local pass is not release qualification, and a cancelled run is not a pass.

### Release qualification and publication gates

Release preparation runs the affected product checks below plus its complete
feature, downstream, fuzz, compatibility, and performance qualification. The routine tier
does not bypass existing pre-push hooks or change hosted/scheduled CI selection.

```sh
cargo fmt --all --check
cargo clippy -p <affected-package> --all-targets
cargo test -p <affected-package>
cargo deny check
```

Before pushing a source, dependency, API, FFI, or workflow change, run
`scripts/ci-act.sh pre-push` from a clean, committed worktree. It executes complete
affected-package and downstream-consumer suites from the checked-in GitHub Actions
workflow in rootless Podman. The selector includes optional/dev/build/platform dependencies,
detached validation workspaces, and shared fixtures; do not replace it with ad-hoc path filters.
Dependency-only manifest/lockfile deltas select users and old/new consumers, including
shared feature users. Shared build policy, central CI changes, unknown ownership, and
unavailable dependency-impact evidence select full routine coverage.
Use `pre-push --base REF` for an explicit comparison and `pre-push --full` for all routine checks.
Missing trunk history also selects full routine coverage. Full routine CI runs nightly;
ordinary prose-only changes run policy checks without Rust builds or fuzzing.
Fuzzing, packaging, compatibility and exhaustive feature/MSRV/cross-target qualification
are release-only and scoped independently to enabled products and their affected closure.
Use `scripts/ci-act.sh release --product NAME` for explicit qualification (never publication).
See [CI policy](docs/CI.md).

Set `ACT_CONCURRENT_JOBS` to limit local container load; hosted CI remains the final authority.
Linked worktrees are validated through a retained, self-contained temporary clone because
the selector needs Git history inside the container. Never publish from that validation clone.
