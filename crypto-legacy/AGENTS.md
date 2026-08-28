# rsl-crypto-legacy

> Inherits the workspace-root `../AGENTS.md` and the evidence discipline in `../crypto/AGENTS.md`.

This package contains historically accurate cryptography that is weak, broken, deprecated, or
otherwise unsuitable for new protection. Correct output must never be described as security.

## Rules

- `#![no_std]`, `#![forbid(unsafe_code)]`; `alloc` may be used deliberately.
- Every concrete algorithm exports a `SecurityStatus` of `Legacy`, `Broken`, or
  `EducationalOnly`; `Recommended` is prohibited in this package.
- Every algorithm page begins with the security failure/deprecation and then teaches the mechanics.
- Every algorithm records its original controlling publication and current deprecation/attack
  sources in `STANDARDS.md`.
- Published known-answer, boundary/malformed, intermediate where available, and differential
  evidence are required before public usability is claimed.
- Secret-bearing owners follow `rsl-crypto`'s non-`Clone`, redacted, zeroizing policy.
- This package never defines cipher-suite negotiation, fallback, record padding, MAC ordering,
  protocol version policy, or downgrade behavior. Those remain in protocol crates.
- No default facade bundle includes this package. Every consumer must opt into `legacy-crypto` or
  depend on `rsl-crypto-legacy` directly.
- APIs should permit exact historical reproduction when required, but documentation and types
  must make dangerous use unmistakable.
