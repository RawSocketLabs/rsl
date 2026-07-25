# Testing Techniques and Tool Selection

### TEST-PROP-001 Derive properties from the domain contract

- **Strength:** SHOULD
- **Applies to:** broad input spaces, algebraic laws, parsers, codecs, numeric
  transforms, state machines, and interchangeable implementations
- **Directive:** State the invariant and generator domain before choosing a
  property-testing crate. Generate valid, boundary, and deliberately invalid
  values as distinct strategies; preserve minimized regressions; and avoid
  tautologies that call the same implementation on both sides.
- **Why:** High input volume does not add evidence when the property merely
  restates implementation behavior or excludes difficult states.
- **Exceptions:** Exhaustive enumeration is better for a genuinely small state
  space. Hand-picked known answers remain necessary for external conformance.
- **Mechanical owner:** Property tests, shrink-regression fixtures, and review.
- **Sources:** Preferences R50-R55 and R133-R139.

### TEST-FUZZ-001 Keep fuzz targets persistent and bounded

- **Strength:** MUST
- **Applies to:** hostile parsers, unsafe boundaries, state machines,
  decompressors, and input-dependent allocation or recursion
- **Directive:** Keep persistent fuzz targets for stable attack surfaces, bound
  memory and work under arbitrary input, seed them with specification vectors,
  boundary cases, licensed or internally owned captures, and minimized
  regressions, and make a short smoke run executable separately from sustained
  fuzzing. Treat a crash, panic, timeout, OOM path, or violated invariant as a
  regression according to repository policy.
- **Why:** One-time fuzzing loses learned failures and cannot enforce parser
  robustness as code evolves.
- **Exceptions:** A closed fixed-size total parser may use exhaustive or
  property tests when they cover its complete input space.
- **Mechanical owner:** Fuzz target compilation, smoke run, scheduled sustained
  run, corpus provenance, and regression tests.
- **Sources:** Preferences R51, R54, R134, and R200; protocol resource rules.

### TEST-TOOLS-001 Classify specialized tools by applicability

- **Strength:** MUST
- **Applies to:** repository validation planning and completion reporting
- **Directive:** Classify formatting, check, Clippy, unit/integration tests, and
  required doctests as ordinary repository checks when adopted. Activate Miri
  for applicable unsafe and aliasing behavior; sanitizers for native memory,
  thread, and FFI behavior; Loom for tractable synchronization models; fuzzing
  for hostile input; `cargo deny` or audit tools for adopted supply-chain
  policy; SemVer checks for compatible public releases; feature matrices for
  meaningful configurations; coverage for evidence gaps; Criterion or
  iai-callgrind for relevant metrics; mutation testing for test strength; and
  snapshots only for intentionally reviewed output.
- **Why:** Running every tool everywhere wastes time, while omitting a relevant
  specialized tool leaves a different risk untested.
- **Exceptions:** A tool may be unavailable locally or inappropriate for the
  target; report it as unavailable or inapplicable and preserve the exact later
  command.
- **Mechanical owner:** Repository validation configuration and dispatcher
  status.
- **Sources:** Preferences R50-R62, R72, R133-R139.

## Technique matrix

| Risk | Primary evidence | Common supplement |
|---|---|---|
| Public behavior | Integration tests and doctests | SemVer checks |
| Algebra/state space | Property tests | Exhaustive small models |
| Hostile bytes | Fuzzing and boundary tests | Sanitizers |
| Unsafe/aliasing | Miri | Sanitizers and compile-fail tests |
| Synchronization | Deterministic model or Loom | Soak/load tests |
| Protocol conformance | Independent known answers | Interoperability and properties |
| DSP correctness | Numeric oracle and chunking equivalence | Captured fixtures |
| Performance | Representative before/after benchmark | Profiler and counters |
| Supply chain | Deny/audit/license workflow | Graph inspection |
| Target behavior | Native target or hardware tests | Cross-build/emulator |

Do not count unavailable tools as passed, and do not let coverage percentage or
a green all-features build substitute for the affected property.
