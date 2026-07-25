---
name: rust-dsp
description: Engineer and review Rust DSP, SDR, numeric kernels, sample buffers, streaming stages, rate changes, discontinuities, completion, timing, and signal evidence. Use for signal-processing or real-time streaming domains. Do not infer DSP semantics from container types alone.
---

# Engineer Rust DSP

## Establish the signal contract

1. Read `$rust-core`, the repository glossary, numeric model, sample formats,
   supported targets, state lifecycle, latency and throughput requirements,
   scalar or reference implementation, and captured or synthetic fixtures.
2. Read [DSP and streaming](references/dsp-and-streaming.md) for vocabulary,
   metadata, discontinuities, processing composition, rate changes, completion,
   and timing.
3. Identify units, ranges, precision, tolerances, NaN/overflow policy, ownership,
   consumed and produced counts, retained state, reset, flush, and errors.

## Design and route

Prefer concrete or static composition until real runtime heterogeneity requires
a trait object. Keep simple finite buffers data-only, but bind continuity,
position, rate, channel, and discontinuity metadata at boundaries that claim
those semantics. Preserve a clear scalar or specification-shaped reference path.

Route measurement and SIMD to `$rust-performance`, buffers and task delivery to
`$rust-async-concurrency`, unsafe kernels to `$rust-unsafe-ffi`, wire recovery to
`$rust-protocol`, and evidence to `$rust-testing`.

## Verify

Use numeric known answers, properties, chunking equivalence, boundary and
alignment cases, discontinuity/reset equivalence, rate and latency mapping,
finite completion, received-versus-recovered evidence, scalar-versus-optimized
equivalence, and representative benchmarks.

## Output

Record vocabulary, numeric contract, state and metadata lifecycle, reference
evidence, tolerance, performance evidence, platform coverage, and limitations.
