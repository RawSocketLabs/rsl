---
name: rust-dependencies-security
description: Add or review Rust dependencies, Cargo features, licenses, advisories, provenance, build scripts, procedural macros, native code, and supply-chain risk. Use for manifest, lockfile, feature, or dependency-surface changes. Do not infer approval merely because a crate is popular.
---

# Review Rust Dependencies and Supply Chain

## Inspect the graph

1. Read `$rust-core`, repository dependency and license policy, MSRV, target and
   feature matrix, manifests, lockfiles, deny configuration, and any approved
   dependency facade.
2. Read [dependency and feature rules](references/dependencies-and-change.md).
   Read [security engineering](references/security.md) when assets, hostile
   actors, privileges, secrets, cryptography, or sensitive diagnostics are
   implicated.
3. State why existing code or dependencies cannot meet the need, who owns the
   new contract, and whether the change is material.

## Assess the change

Inspect exact source and version, maintenance and release posture, transitive
graph, duplicate versions, default and optional features, MSRV, licenses,
advisories, yanks, build scripts, procedural macros, native code, unsafe,
network/build access, cryptography, serialization, platform support, and public
API exposure. Name features for their truthful public contract.

Do not add wildcard or unreviewed Git dependencies. Do not silently enable
default features or expand a public feature graph. Treat a lockfile-only change
as a real resolved-graph change.

Route public compatibility to `$rust-api-design`, feature tests to
`$rust-testing`, unsafe/native boundaries to `$rust-unsafe-ffi`, and embedded
constraints to `$rust-embedded`.

## Verify

Run repository-declared graph, license, advisory, and feature-matrix checks such
as `cargo tree`, `cargo metadata`, `cargo deny`, or `cargo audit` when available.

## Output

Report necessity, alternatives, graph and feature impact, provenance, license,
advisory status, MSRV/target impact, commands observed, and approval state.
