# rsl-compression

> Inherits the workspace-root `../AGENTS.md`.

Accuracy-first compression contracts and, later, reference algorithms. Preserve streaming state,
dictionary resets, flush behavior, exact input/output counts, and malformed-input errors
explicitly. Do not silently accept truncated streams or conflate decompression with validation.

## Rules

- `#![forbid(unsafe_code)]`, `#![no_std]`, `alloc` available.
- Prefer small, specification-shaped functions and named intermediate values.
- Require published vectors, boundary tests, malformed-input tests, and differential tests for
  every concrete algorithm before it is advertised as usable.
- Keep the readable reference path intact if optimized implementations are added later.
- Compression is an independent domain. Do not make it implement a universal transform contract
  shared with cryptography or error correction.
