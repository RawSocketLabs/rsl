---
name: rust-embedded
description: Engineer and review no-std, firmware, interrupt, DMA, peripheral, allocator, panic, binary-size, timing, and hardware-targeted Rust. Use for embedded or target-constrained repositories. Do not claim hardware behavior from host-only compilation.
---

# Engineer Embedded Rust

## Establish the target contract

1. Read `$rust-core`, exact target triples and hardware revisions, linker and
   memory scripts, toolchain, `no_std` and allocation policy, HAL/PAC ownership,
   interrupt model, panic strategy, bootloader, flashing, and hardware CI.
2. Read [embedded contracts](references/embedded.md).
3. Identify which behavior is target-independent, simulator-testable,
   architecture-specific, board-specific, timing-sensitive, or safety-critical.

## Design

Keep core logic host-testable where possible. Make allocator and heap use,
blocking, interrupt masking, critical sections, atomics, DMA buffer ownership,
alignment, cache coherency, volatile access, peripheral state, power/reset, and
panic behavior explicit. Avoid hidden `std`, runtime, filesystem, thread, or
wall-clock assumptions.

Route API contracts to `$rust-api-design`, target features and dependencies to
`$rust-dependencies-security`, interrupts and shared state to
`$rust-async-concurrency`, unsafe register/ABI work to `$rust-unsafe-ffi`,
measurement to `$rust-performance`, and evidence to `$rust-testing`.

## Verify

Run host tests for portable logic and real target builds for every first-class
configuration. Use emulator or hardware tests, interrupt/race tests, size and
stack checks, timing measurements, and fault/panic/reset tests as required.
Separate compiled, emulated, flashed, and observed-hardware evidence.

## Output

Record capability contract, target matrix, allocation and panic policy, timing
and size budgets, unsafe/interrupt boundaries, exact evidence, unavailable
hardware, and residual risks.
