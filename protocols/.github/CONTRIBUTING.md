# Contributing

Thanks for your interest. This is a product-first, maintainer-decides workspace; for anything
non-trivial, **open an issue first** so we can agree on scope before you write code.

## Workflow

- **One concern per change**, on a branch off `main` → PR → green CI → squash-merge.
- **Conventional Commits**, scope = the crate/protocol name (`feat(dns): …`). The breaking marker
  is `type(scope)!:` — after the scope, never `type!(scope):`. commitlint enforces this on every
  commit in a PR; release-plz derives versions from it. See [`../VERSIONING.md`](../VERSIONING.md).
- **All CI gates must pass**: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets`
  (`clippy::all` is denied), `cargo build`/`test --workspace`, `cargo deny check`, and the MSRV
  (1.85) check. Run them locally before pushing.

## House rules

- **Dual-use is non-negotiable** (see [`../AGENTS.md`](../AGENTS.md)): never make a parser reject
  representable input; model unknowns as `Custom(..)`; compliance lives on the default path, not
  in the parser.
- **The codec is `bnb`** — prefer `#[bin]`/`#[bitfield]`/`#[derive(BitEnum)]`/`#[bitflags]` and
  `parse_with`/`#[bin(codec = …)]` for custom shapes over hand-rolled byte work. If bnb can't
  express something cleanly, that's a bnb finding — raise it, don't paper over it.
- **Public items are documented** (`missing_docs`); a clean crate runs `#![deny(missing_docs)]`.
- **Inbound = outbound**: contributions are dual-licensed MIT OR Apache-2.0.
