---
name: rust-api-design
description: Design and review Rust APIs, type invariants, ownership, traits, conversions, errors, documentation, compatibility, builders, and feature-exposed surfaces. Use when public or durable internal contracts change. Do not activate for purely local implementation details with no lasting interface consequence.
---

# Design Rust APIs

## Inspect the contract

1. Read `$rust-core`, the repository compatibility policy, MSRV, feature model,
   public callers, existing types, documentation, and downstream examples.
2. Identify who establishes and preserves each invariant, what invalid states
   remain representable, and whether the API is public, serialized, FFI, wire,
   generated, or layout-sensitive.
3. Read [API, ownership, and errors](references/api-ownership-and-errors.md) for
   type and trait decisions. Read
   [ownership patterns](references/ownership-patterns.md) for state modeling,
   validated construction, exact quantities, borrowed options, frozen shared
   sequences, builders, type-state, and error-library boundaries.
   Read
   [documentation and examples](references/documentation-and-examples.md) when
   the public teaching or compatibility surface changes.

## Design and route

- Prefer familiar concrete APIs until demonstrated callers need genericity,
  dynamic dispatch, type-state, or a macro.
- Make fallibility, ownership transfer, allocation, panic, and error loss
  explicit.
- Treat a repeated runtime guard as a modeling question: decide whether the
  state belongs in an enum variant, a validated constructor, or a type
  parameter, and delete the checks the chosen type proves.
- Treat builders, newtypes, enums, traits, and validated wrappers as tools for
  real invariants, not universal patterns.
- Route tests and SemVer evidence to `$rust-testing`, Cargo feature and
  dependency impact to `$rust-dependencies-security`, and domain semantics to
  the applicable skill.

## Verify

Compile public examples and doctests. Exercise downstream call syntax, feature
configurations, error paths, mutation invariants, and compatibility tooling when
required. Record any deliberate breaking change and migration path.

## Output

State the API contract, alternatives considered, compatibility and MSRV impact,
error and panic behavior, documentation updates, tests, and unresolved risks.
