---
name: rsl-rust-core
description: Guide material Rust implementation, refactoring, API, dependency, concurrency, unsafe, testing, documentation, performance, protocol, and DSP changes under repository-specific profiles. Use when changing Rust code or design and ownership, misuse resistance, evidence, or local constraints matter. Do not use for a trivial syntax explanation or unrelated non-Rust work.
---

# RSL Rust Core

## Apply precedence

Honor the current user request, then the closest repository instructions, parent
instructions, adopted domain guidance, this skill, and general defaults. Surface
a material conflict. Never replace a repository fact with a global preference.

## Work through the change

1. Inspect applicable instructions, `rsl-rust-standards.toml`, manifests, nearby
   code and tests, generated boundaries, and relevant history.
2. Identify the repository profile, affected contract, trust boundary, hot path,
   platform constraints, and what is uncertain. If no profile is declared, state
   a conservative assumption and confine its blast radius.
3. Choose the smallest correct design that keeps normal use clear and difficult
   to misuse. Make ownership, errors, overload, lifecycle, and escape hatches
   explicit where they matter.
4. Read only the references implicated by the change.
5. Implement the requested scope without speculative rewrites or silent adjacent
   cleanup. Discuss every material dependency change before making it.
6. Run the repository's declared default verification plus risk-specific
   evidence. Measure performance claims and test native behavior where target-
   specific code matters.
7. Review the completed diff for correctness, accidental API expansion, panic,
   allocation, unsafe, documentation, example, and unrelated-change regressions.
   Report what ran, what did not, and why.

## Load references selectively

- Read [profiles and priorities](references/profiles-and-priorities.md) when
  selecting tradeoffs or when the repository profile is absent or overridden.
- Read [API, ownership, and errors](references/api-ownership-and-errors.md) for
  public surfaces, data movement, error contracts, concurrency, unsafe code, or
  performance-sensitive ownership.
- Read [dependencies and change](references/dependencies-and-change.md) for Cargo
  features, dependencies, MSRV, source policy, generated files, or scope choices.
- Read [verification](references/verification.md) when choosing tests, fuzzing,
  platform evidence, benchmarks, or completion evidence.
- Read [documentation and examples](references/documentation-and-examples.md)
  when public behavior, concepts, guides, rustdoc, or `examples/` change.
- Read [style](references/style.md) when writing or materially restructuring Rust
  control flow, names, modules, unsafe blocks, or lint exceptions.

## Preserve hard boundaries

- Do not introduce a production panic path except for a documented invariant
  whose violation makes continued execution impossible.
- Do not claim zero-copy, constant time, throughput, latency, or improvement
  without a defined workload and evidence.
- Deny unsafe by default; require a contained proof obligation and applicable
  verification when a repository explicitly permits it.
- Keep memory safety independent of protocol validity. An escape hatch may create
  invalid wire data, never undefined behavior.
- Let the application own optional async and parallel runtimes. Do not impose
  Tokio, Rayon, a global pool, or hidden scheduling through a base library.
