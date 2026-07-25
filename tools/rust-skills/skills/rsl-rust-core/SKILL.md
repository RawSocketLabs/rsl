---
name: rsl-rust-core
description: Compatibility router for repositories that previously activated the RSL Rust core package. Use only while migrating a legacy installation, routing the work to the canonical modular rust-core and the applicable task or capability skills. Do not install it in a new repository or treat it as a source of engineering rules.
---

# RSL Rust Core Compatibility

This legacy identity no longer owns portable rules. Apply `$rust-core`, then
activate `$rust-implement` for changes or the applicable API, testing, protocol,
DSP, performance, async/concurrency, unsafe/FFI, dependency/security, or
embedded skill.

Preserve repository-local RSL decisions and profiles above portable guidance.
Do not copy legacy instructions into a new repository. Use
`$rust-repository-onboarding` to inspect, interview, approve, and record a
modular adoption before replacing an installed legacy package.

When reporting work, disclose that this compatibility router was activated and
name the canonical skills actually applied.
