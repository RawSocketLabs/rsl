# Repository Inspection Contract

Inspect before asking questions or proposing profiles. Record facts with paths
and distinguish absence from inability to inspect.

## Purpose and structure

- Read root and nested `AGENTS.md`, `CLAUDE.md`, contributor guides, design
  documents, changelogs, release configuration, and generated-file notices.
- Inventory every `Cargo.toml`, workspace membership and exclusions, packages,
  libraries, binaries, examples, benches, build scripts, proc macros, FFI
  crates, and independently versioned components.
- Classify the repository as public or internal library, application, service,
  firmware, experimental code, or a mixed workspace. Detect protocol, parser,
  DSP, networking, cryptography, async, concurrent, real-time, embedded,
  `no_std`, FFI, platform-specific, performance-sensitive, security-sensitive,
  safety-critical, public-API, and generated-code capabilities.

## Compatibility and tooling

- Inspect edition, `rust-version`, toolchain files, MSRV CI, target triples,
  operating systems, architectures, stable/nightly use, linker configuration,
  `no_std`, allocator and panic policy.
- Read `rustfmt.toml`, workspace and crate lints, Clippy configuration, deny
  files, audit configuration, feature definitions, default features, and
  feature-matrix CI.
- Record canonical format, check, build, lint, test, docs, coverage, Miri,
  sanitizer, Loom, fuzz, dependency, SemVer, benchmark, size, target, and
  packaging commands. Identify authoritative CI jobs.

## Contracts and evidence

- Inventory public exports, error types, panic documentation, builders,
  type-state and newtypes, feature-gated APIs, compatibility promises, unsafe
  blocks, manual `Send`/`Sync`, FFI boundaries, generated sources, examples,
  doctests, fixtures, snapshots, property tests, fuzz targets, benchmarks, and
  known-answer or interoperability vectors.
- Inspect dependencies, sources, versions, features, duplicate versions,
  licenses, advisories, build scripts, procedural macros, native code, Git
  dependencies, and organization dependency facades.
- Locate hot paths and any documented latency, throughput, memory, allocation,
  binary-size, compile-time, or target-hardware budgets. Do not infer a path is
  hot merely from low-level code.

## Domain inspection

For protocols, record specifications and revisions, bit and byte conventions,
raw versus semantic types, builder and parse validation, resource limits,
partial and malformed outcomes, unknown and reserved values, integrity,
correction, framing, state machines, and vector provenance.

For DSP and streaming, record the repository glossary, sample and buffer types,
rate and unit types, processor contracts, state/reset/finish behavior, metadata,
epochs, discontinuities, received and recovered evidence, reference
implementations, tolerances, and hardware fixtures.

For concurrency, inventory every queue and spawned-work class, capacity unit,
retained bytes, rate assumptions, overload behavior, runtime or pool, owner,
handle, cancellation, shutdown, cleanup, result, panic observation, and
detachment.

## Inspection output

Produce a fact table with source locations, a proposed base and capabilities,
conflicts or stale instructions, unavailable evidence, and the smallest set of
questions that source inspection cannot answer.
