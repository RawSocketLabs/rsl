---
name: rust-async-concurrency
description: Design and review Rust async and concurrent ownership, tasks, cancellation, channels, queues, backpressure, overload, synchronization, parallelism, and shutdown. Use when work crosses tasks, threads, runtimes, pools, or suspension points. Do not impose an async runtime on synchronous libraries without repository approval.
---

# Engineer Async and Concurrent Rust

## Establish ownership and rates

1. Read `$rust-core`, runtime and target policy, producer and consumer rates,
   queue capacities, retained bytes, latency assumptions, overload behavior,
   cancellation points, and shutdown ownership.
2. Read [concurrency contracts](references/concurrency.md).
3. For each spawned work class, name the owner and handle, admission stop,
   shutdown signal, drain or discard decision, resource return, bounded join,
   timeout escalation, result and panic observer, and any approved detachment.

## Design

Prefer returning a future, stream, or driver for the caller to own. Keep optional
runtimes and Rayon pools application-owned. Specify queue semantics before
selecting a primitive; centralize composite drop, coalescing, recycling,
metadata, and shutdown behavior behind a narrow domain type.

Trace every `select`, timeout, abort, cancellation, or future race through the
losing operation and partial side effects. Avoid locks across suspension,
callbacks, blocking work, or uncontrolled operations.

Route correctness schedules to `$rust-testing`, rates and cutoffs to
`$rust-performance`, unsafe synchronization to `$rust-unsafe-ffi`, stream-loss
meaning to `$rust-dsp`, and framing recovery to `$rust-protocol`.

## Verify

Test clean, busy, overloaded, cancelled, panicking, and missed-deadline paths.
Use Loom, deterministic models, soak, or load tests when appropriate.

## Output

Report the ownership graph, queue budget, cancellation safety, shutdown result,
runtime requirements, and observed evidence.
