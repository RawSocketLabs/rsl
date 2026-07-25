# Name Features for Runtime Commitments

## Before

```toml
[features]
fast = ["dep:tokio"]
```

The name hides an ecosystem and runtime requirement.

## Review

Callers cannot infer that enabling `fast` exposes Tokio types, requires a Tokio
context, or changes task ownership. “Fast” is also an unmeasured promise.

## After

```toml
[features]
tokio = ["dep:tokio"]
```

If the feature provides a neutral capability through a runtime-independent API,
name that capability and keep Tokio private only when callers inherit no Tokio
types, context, or observable scheduling semantics.

## Tests

Build default features, the individual feature, documented interactions, and
all-features as a supplement. Inspect `cargo tree -e features`, public docs,
MSRV, and SemVer impact.

## Lesson

A Cargo feature is public configuration. Its name must tell the truth about the
API, ecosystem, runtime ownership, or behavior it enables.

## Applies when

- A feature exposes or requires a concrete runtime or dependency ecosystem.
- Optional dependency features become part of a public crate contract.

## Does not apply when

- A genuinely neutral capability has multiple hidden implementations with no
  caller-visible ecosystem coupling.
