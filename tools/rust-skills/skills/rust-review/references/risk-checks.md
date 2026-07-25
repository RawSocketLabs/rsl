# Risk Routing

Activate only categories implicated by the diff. Use the named skill as the
single detailed owner.

| Diff signal | Activate | Review routing question |
|---|---|---|
| Public type, trait, conversion, builder, error, docs, feature-exposed API | `$rust-api-design` | Did invariants, caller syntax, errors, compatibility, and documentation change truthfully? |
| Tests, fixtures, examples, properties, fuzzing, snapshots, CI evidence | `$rust-testing` | Does evidence target the changed property and report unavailable layers honestly? |
| Wire bytes, parser, encoder, framing, CRC/FEC, malformed input, protocol state | `$rust-protocol` | Does behavior follow the pinned authority, explicit representation, bounded parsing, and independent vectors? |
| Samples, numeric kernel, streaming state, rate, discontinuity, receiver stages | `$rust-dsp` | Are units, numeric tolerance, metadata, state lifecycle, and reference equivalence preserved? |
| Latency, throughput, allocation, cache, SIMD, branches, code or compile size | `$rust-performance` | Is the real workload measured before and after with correctness preserved? |
| Async, tasks, threads, queues, locks, channels, pool, timeout, shutdown | `$rust-async-concurrency` | Are ownership, cancellation, overload, cleanup, and bounded completion explicit? |
| Unsafe, raw pointers, manual `Send`/`Sync`, ABI, callbacks, native handles | `$rust-unsafe-ffi` | Is the complete safety/ABI proof present and impossible for safe callers to violate? |
| Cargo manifests, lockfile, features, licenses, advisories, crypto, build code | `$rust-dependencies-security` | Is the change necessary, approved, provenance-reviewed, and tested across its real graph? |
| `no_std`, firmware, interrupts, DMA, linker, board, target-only code | `$rust-embedded` | Is target ownership explicit and is target or hardware evidence distinguished from host evidence? |
| Generated sources, rule IDs, adapters, skill manifests, evals | `$rust-skill-maintenance` | Was the canonical owner edited with migration, generation, and evaluation integrity? |

## Always check

- Correctness before idiomaticity and style.
- Panic, error loss, resource growth, data loss, and cleanup on reachable paths.
- Accidental API, feature, dependency, target, wire, persistence, or ABI change.
- Diff scope and generated ownership.
- Exact command truthfulness.

## Reject low-value comments

Do not request iterator chains merely for compactness, abstraction without a
demonstrated boundary, removal of every clone, removal of test `unwrap`,
rustfmt-managed style, speculative optimization, or cleanup unrelated to the
requested change. Do not impose numeric nesting or function-size limits, remove
every `else`, expand established domain abbreviations, delete durable comments,
or require an interface solely for test doubles. Optional suggestions belong
after findings and must identify their concrete benefit.
