# rsl-error-correction

> Inherits the workspace-root `../AGENTS.md`.

Accuracy-first forward-error-correction contracts and, later, reference algorithms. A decoder
must distinguish clean, corrected, and uncorrectable inputs. Never return corrected bytes without
the accompanying correction report.

## Rules

- `#![forbid(unsafe_code)]`, `#![no_std]`, `alloc` available.
- Prefer small, specification-shaped functions and named intermediate values.
- Require published vectors, correction-boundary tests, malformed-input tests, and differential
  tests for every concrete code before it is advertised as usable.
- Keep the readable reference path intact if optimized implementations are added later.
- Error correction is an independent domain. Do not make it implement a universal transform
  contract shared with cryptography or compression.
