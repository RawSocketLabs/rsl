---
name: rsl-rust-review
description: Compatibility router for repositories that previously activated the RSL Rust review package. Use only during legacy migration, routing every review to the canonical rust-review skill plus the affected domain and technique skills. Do not install it in a new repository or rely on it for review criteria.
---

# RSL Rust Review Compatibility

Apply `$rust-review` for finding admission, severity, confidence, evidence, and
reporting. Activate the changed API, testing, protocol, DSP, performance,
async/concurrency, unsafe/FFI, dependency/security, or embedded owner rather
than relying on a duplicated legacy checklist.

Do not modify code during a review unless the user separately requests a fix.
Use `$rust-repository-onboarding` to migrate the repository after its local
rules, profile, adapter family, and validation workflow are approved.
