---
name: rust-implement
description: Implement material Rust changes under repository-local policy with scoped edits, capability routing, tests, and truthful validation. Use when asked to write, fix, refactor, or extend Rust code. Do not use for review-only requests or trivial syntax answers.
---

# Implement Rust Changes

## Inspect before editing

1. Read `$rust-core`, local instructions, manifests, nearby code and tests,
   generated ownership, feature gates, and relevant history.
2. State the requested behavioral outcome and smallest affected contract.
3. Route public API, testing, protocol, DSP, performance, concurrency,
   unsafe/FFI, dependency/security, and embedded concerns to their owning
   skills before designing the patch.

## Implement

- Preserve existing local conventions unless the task explicitly changes them.
- Keep the diff focused; do not mix unrelated cleanup or dependency churn.
- Prefer clear control flow and domain vocabulary. Read
  [implementation style](references/style.md) for non-mechanical choices.
- Update tests, public documentation, examples, generated sources, fixtures,
  benchmarks, and migration notes when their owning contract changes.
- Discuss a material dependency change before editing manifests unless the user
  already authorized that exact change.

## Validate and self-review

Run the repository-declared required workflow and risk-specific checks selected
by `$rust-testing`. Review the completed diff for behavioral regression,
accidental public surface, panic, allocation, unsafe, feature, target,
documentation, and scope changes. Never describe an unavailable tool as passed.

## Output

Deliver the scoped implementation, tests, exact command results, skipped or
unavailable checks, behavior changes, compatibility impact, and remaining risk.
