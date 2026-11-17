# crypto/os-random

The operating system's random generator as a `crypto::RandomSource`.

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The workspace root
> [`AGENTS.md`](../../AGENTS.md) also applies. `crypto/core/AGENTS.md` does not: its rules on
> published vectors and the standards ledger are for algorithms, and this crate implements
> none.

## What it is

One type, `OsRandom`, which fills from the operating system's cryptographic generator through
the `getrandom` crate. `crypto` defines the `RandomSource` boundary and names no platform
generator (`crypto/core/src/random.rs`); this crate is where that choice is made and reviewed.

## Rules

- `#![no_std]`, `#![forbid(unsafe_code)]`. The `unsafe` and the system calls are in `getrandom`.
- **Stateless.** No buffering, no reseeding logic, no user-space generator. A buffered source
  is where fork and snapshot bugs live; if one is ever wanted it is a different type.
- **It waits rather than degrades.** Never add a non-blocking or "insecure" fallback.
- **Callers inject it.** Protocol crates take `&mut impl RandomSource` and never construct
  `OsRandom` themselves, so their tests stay reproducible. Only binaries, examples and
  convenience constructors name it. `cargo tree -i os-random` is the list of code that draws
  operating-system randomness.
- Do not re-export it from the `rsl` facade until asked: the path would become API.
- The backend is `getrandom` with default features off. Do not set a `getrandom_backend` cfg in
  this repository.

## Testing

Inline `unit` tests only. They prove the adapter fills what it is given: every length
succeeds, every position of a long buffer is written across several draws, and two draws
differ. They do not, and must not, assert anything statistical, anything about timing, or the
buffer's contents after an error.

Run: `cargo test -p os-random`.
