# Adaptive Repository Interview

Ask coherent rounds of roughly five to ten questions. Skip answered facts,
explain conflicts found in source, summarize decisions after each round, and
adapt later questions. Do not finalize local rules until the owner approves the
complete proposal.

## Round 1: purpose and consumers

Ask what the repository does; whether it is internal, public, experimental,
production, security-sensitive, safety-critical, or regulated; who consumes it;
whether it is a library, binary, service, firmware component, or mixed
workspace; expected maintenance lifetime; and whether components need different
profiles.

## Round 2: compatibility and platforms

Confirm edition, MSRV and support window, stable or nightly policy, toolchain
pins, operating systems, architectures, hardware, `no_std`, allocation, panic,
and backward-compatibility commitments. Ask whether pre-1.0 breaking changes are
allowed and how they are announced and tested.

## Round 3: APIs, errors, and maintainability

Ask which APIs are public; preferred use of custom errors, `thiserror`,
`anyhow`, boxed errors, and source preservation; acceptable panic and `unwrap`;
builders, type-state, newtypes, and `#[non_exhaustive]`; feature-gated APIs;
readability versus compactness; loops versus iterator chains; clarity-motivated
clones; generics, traits, enums, and dynamic dispatch; macro and proc-macro
policy; documentation level; module organization; and focused-change scope.

## Round 4: performance and execution

Ask for known measured hot paths, workload sizes and frequencies, latency,
throughput, memory, allocation, binary-size, and compile-time budgets; buffer
reuse; unsafe and SIMD permission; reference implementation requirements;
benchmark tools; async runtime ownership; caller-owned parallel pools; queue
capacity and overload; cancellation; shutdown; blocking; and observability.

## Round 5: testing and validation

Confirm required unit, integration, doctest, example, property, fuzz, snapshot,
mutation, known-answer, interoperability, Miri, sanitizer, Loom, coverage,
SemVer, feature-matrix, MSRV, target, benchmark, and hardware tiers. Ask which
commands are authoritative, which tools are unavailable locally, acceptable CI
duration, flake policy, fixture provenance, and whether missing required tools
must fail adoption validation.

## Round 6: protocols and binary data

When applicable, ask for governing specifications and errata, citation style,
bit numbering, wire byte order, symbol/dibit/bit/octet vocabulary, raw versus
semantic types, unknown and reserved behavior, strict and relaxed validation,
parser resource budgets, incomplete/malformed consumption, resynchronization,
zero-copy goals, received and recovered evidence, CRC/checksum/FEC behavior,
known-answer vectors, round-trip limits, fuzzing, and external interoperability.

## Round 7: DSP, streaming, unsafe, and FFI

When applicable, confirm domain vocabulary, units, sample and channel geometry,
rate relationships, tolerances, NaN/overflow policy, chunk/block/dwell meaning,
stream epochs, discontinuities, metadata placement, reset and finish behavior,
tail policy, reference kernels, captured signals, and hardware tests.

For unsafe and FFI, ask whether unsafe is forbidden, audited, or documented;
which ABIs and foreign versions apply; ownership, callbacks, thread affinity,
panic/unwind, linking, and target testing; and whether Miri or sanitizers are
feasible.

## Round 8: dependencies, security, and workflow

Ask how conservative additions should be; license restrictions; registry, Git,
wildcard, build-script, proc-macro, native, crypto, and security-sensitive
dependency policy; `cargo deny` and audit requirements; generated-file
ownership; changelog and design-note expectations; review finding schema and
severity; required commands; adapter family and installation mode; and how
unresolved decisions are recorded.

## Final confirmation

Present verified facts, recommended base and capabilities, local overrides,
required and optional validation, skill set, generated files, conflicts,
deferred decisions, and provenance. Obtain explicit approval before writing.
