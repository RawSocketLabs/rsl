# Modular Rust Engineering System

## Status

The owner approved this architecture on 2026-07-24. All 14 canonical portable
packages are implemented. Catalog, profile, pack, reference, example,
organization, migration, onboarding, repository-validation, and evaluation
contracts are executable. `rsl-rust-core` and `rsl-rust-review` remain
installable only as thin compatibility routers and are excluded from default
new-repository installation.

## Layers

1. Portable, model-neutral `rust-*` skills own reusable Rust judgment.
2. Organization manifests and indexed decision records preserve organization
   preferences without embedding them in portable guidance.
3. Repository profiles combine one semantic base with zero or more
   capabilities.
4. Repository decisions and existing local instructions override profiles and
   organization preferences.
5. Agent-specific adapters change discovery and interface metadata, not
   engineering policy.
6. Executable workflows report what ran and distinguish passed, failed,
   skipped, unavailable, and inapplicable checks.
7. References and curated examples provide attributed evidence below explicit
   local decisions.
8. Evaluations measure activation, output quality, command truthfulness, and
   regressions.

## Skill catalog

| Skill | Ownership |
|---|---|
| `rust-core` | Universal priorities, precedence, evidence, and engineering judgment |
| `rust-implement` | Scoped implementation workflow and capability routing |
| `rust-review` | Review procedure, finding schema, severity, and confidence |
| `rust-repository-onboarding` | Inspection, interview, approval, configuration, and installation |
| `rust-api-design` | APIs, types, errors, documentation, compatibility, and SemVer |
| `rust-testing` | Tests, properties, fuzzing, fixtures, snapshots, and verification |
| `rust-protocol` | Wire models, parsing, encoding, framing, malformed data, and conformance |
| `rust-dsp` | Numeric and streaming DSP contracts, reference forms, and signal evidence |
| `rust-performance` | Measurement, allocation, memory, cache, SIMD, and benchmarks |
| `rust-async-concurrency` | Ownership, tasks, cancellation, backpressure, synchronization, and shutdown |
| `rust-unsafe-ffi` | Unsafe invariants, safe boundaries, ABI contracts, and FFI wrappers |
| `rust-dependencies-security` | Dependencies, licenses, advisories, build surfaces, and supply chain |
| `rust-embedded` | `no_std`, targets, allocators, interrupts, firmware, and hardware validation |
| `rust-skill-maintenance` | Authoring, references, generation, migrations, evals, and releases |

The catalog deliberately keeps errors and documentation in API design, property
testing and fuzzing in testing, concurrency with async, FFI with unsafe review,
and backend-neutral observability in core. Split them only when independent
activation and ownership are demonstrated by real tasks and evals.

`rsl-rust-core` and `rsl-rust-review` are indexed separately in
`catalog/compatibility-skills.toml`. They hold their own identities, own no
rules, join no pack, and are validated for drift like any other package.

## Relations and ownership

Each manifest declares:

- `requires`: inherited instruction ownership; this graph must be acyclic;
- `routes_to`: skills to activate conditionally for a task or artifact;
- `related`: navigation only, with no inherited rules.

The ownership direction is:

```text
task orchestrator -> domain skill -> technique skill -> rust-core
```

Orchestrators select skills but do not copy domain rules. Domain skills own
their checklists and route to testing, performance, concurrency, unsafe, or
dependency guidance when those concerns are present. Reciprocal `related` links
are allowed because they do not transfer ownership, and `routes_to` may contain
cycles for the same reason; only `requires` must be acyclic.

Declared relations and runtime prose are one contract. Every `$skill` sigil in a
package's entry file or references must resolve to an installable skill and be
declared by that package, so guidance cannot route somewhere the ownership graph
does not admit.

## Profiles

A repository selects exactly one base:

- `public-library`
- `internal-library`
- `application`
- `service`
- `experimental`

It may add any compatible capabilities from these groups:

- domain: `protocol`, `parser-serializer`, `dsp`, `networking`,
  `cryptography`;
- execution: `async`, `concurrent`, `real-time`;
- environment: `embedded`, `no-std`, `ffi`, `platform-specific`;
- risk: `performance-sensitive`, `security-sensitive`, `safety-critical`;
- structure: `workspace`, `public-api`, `generated-code`.

Presets such as `async-service`, `cli`, `firmware`, `infrastructure`,
`research`, and `mixed-workspace` expand to a base and capabilities. A preset is
a proposed default, not a rigid policy. Capability implication must be
acyclic. Packs select overlapping skill sets and use union-and-deduplicate
semantics.

Mixed workspaces use root defaults plus confirmed component overlays. Every
override records rationale and provenance.

## Precedence

Apply guidance in this order:

1. explicit user instruction for the current task;
2. closest directory-specific instructions and decisions;
3. repository-wide decisions and mechanical configuration;
4. confirmed component and repository-profile defaults;
5. organization-wide preferences;
6. applicable task and capability skills;
7. official authoritative references;
8. approved ecosystem guidance;
9. curated examples;
10. advisory or historical material;
11. general model knowledge.

Higher layers may override lower layers. An apparent override of correctness,
soundness, or safety must be surfaced rather than silently applied.

## Repository onboarding contract

The onboarding skill inspects before asking policy questions. Inspection
covers repository purpose, Cargo manifests, workspace structure, CI, lints,
formatting, toolchains, MSRV, features, tests, documentation, benchmarks,
unsafe code, dependencies, generated files, existing instructions, and any
domain glossary or vocabulary encoded in public types, modules, guides, errors,
tests, and examples. For streaming repositories it distinguishes data-only
finite buffers from continuity-bearing boundaries and inventories which stage
establishes or transforms each metadata field. For lossy streaming repositories
it records stream epochs and index units, the within-epoch known-half-open or
unknown loss extent, the next delivered position, reason vocabulary,
restart/reconfiguration behavior, and repeated-loss accumulation. It also
inventories existing processor traits, their implementors and contracts,
runtime selection points, and whether concrete, generic, enum, or dynamic
dispatch is actually required.
For rate-changing stages it records ratio direction, absolute rates,
state-dependent bounds, latency/reference mapping, and any variable-rate
contract.
For stateful streaming stages it records reset semantics separately from finite
completion, the tail policy, post-completion behavior, synthetic-tail
provenance, and the owner of each live-stream finish/drain or discard/reset
choice.
When timing is instrumented it records the named events and capture points,
clock domains, process-local monotonic representation, exported durations,
external correlation or persistence policy, and any hot-path sampling.
When operational diagnostics exist it records typed evidence, the selected
logging ecosystem, application-owned subscriber and export configuration,
instrument semantics and lifecycle, label cardinality, sensitive-field policy,
optional adapters, and overhead budget.
For concurrent repositories it also inventories queue capacities, payload
bounds, rate and latency assumptions, overload behavior, sample-loss metadata
and state policy where applicable, selected queue implementations and composite
wrapper responsibilities, and each spawned-work class's owner/handle, admission
stop, shutdown signal, drain/discard behavior, resource return, join deadline,
timeout fallback, result/panic observer, and detachment policy.
For protocol builders and parsers it inventories strict defaults, named
validation groups and relaxations, construction-versus-parsing policy,
non-disableable safety, finite parser-budget dimensions, units, scope, reset,
rationale, and approved overrides, post-build validation,
construction-policy exclusion from message identity, validation and correction
evidence states and retention, trusted-input boundaries or wrappers, mutation
invalidation, evidence persistence, exact received-versus-recovered
representation, separate integrity and correction status, integrity or
authentication trust status, complete/incomplete/malformed outcomes, stateless
and stateful byte-consumption contracts, resynchronization authority, and
intentionally invalid encoding.

The agent then:

1. reports verified facts with locations;
2. proposes a base and capability combination;
3. asks adaptive questions in coherent subject rounds, including confirmation
   of local terminology, mappings to shared or organization vocabulary, and
   stream-metadata, discontinuity, processing-composition, rate-relationship,
   streaming-completion, timing-instrumentation, observability, and protocol-
   validation policy where applicable;
4. separates facts, user decisions, organization preferences,
   recommendations, and unresolved questions;
5. presents the full proposed policy and layout;
6. obtains approval before writing;
7. generates a draft and presents its diff;
8. accepts corrections and obtains final confirmation;
9. installs selected adapters and records exact provenance;
10. creates repository-appropriate validation and records deferrals.

For a repository without an established layout, the default is:

```text
AGENTS.md
.rust-skills/
├── adoption.toml
├── repository-profile.md
├── enabled-skills.md
├── validation.toml
├── validation.md
├── decisions/
└── unresolved.md
scripts/
└── validate-rust.py
```

For a mature repository, preserve its existing local rules as canonical.
`.rust-skills/` records provenance, profiles, routing, validation, and links to
those rules without duplicating them.

## References

Reference tiers are `authoritative`, `approved-guidance`,
`curated-examples`, `advisory`, and `historical`. Each manifest records a
revision, license posture, use, limitations, review date, and refresh rule.
Explicit repository decisions outrank every tier.

Repository source is exemplary only for a named aspect at a reviewed revision.
The source catalog never treats a successful repository as uniformly
exemplary.

## Curated examples

Each example records metadata and the sections:

```text
before
review
after
tests
lesson
applies-when
does-not-apply-when
source and provenance
validation
```

The foundation validates the example index, metadata, skill/profile/source
references, provenance, and required transformation sections. The current
corpus contains ten transformations spanning API ownership, builders, errors,
protocol bit order, cancellation, unsafe FFI, reusable buffers, dependency
features, and qualified readability refactoring. Add examples only with
applicability limits and corresponding behavioral evidence.

## Validation and evaluations

The skills repository keeps `cargo xtask validate` as its canonical dispatcher.
Repository adoption may generate a Python dispatcher when that improves
feature-matrix handling, structured output, or portability.

Unavailable tools never count as passed. Human-readable and JSON results are
the standard outputs; JUnit and SARIF are adapters where useful.

The first modular evaluation suite contains 22 cases and grows beyond 30 before
the modular catalog is declared stable. Paired positive and counterexample cases
guard against slogans becoming mechanical rewrites. Schema 2 records expected
skill selection, commands, hard gates, forbidden regressions, hidden-test
interfaces, and external baseline/current result locations. Hard gates include
compilation, tests, hidden tests, protocol vectors, safety reasoning, review
precision/recall, diff scope, local conventions, and truthful command reporting.
Weighted scores cannot hide a hard-gate failure.

## Versioning

- Use `rust-skills-v<semver>` release tags and exact consumer pins.
- Treat skill behavior, schemas, discovery, generated layouts, and precedence as
  versioned contracts.
- Keep stable rule and skill IDs; never recycle a retired identity.
- Mark incompatible pre-1.0 changes and provide migration previews.
- Update canonical content, generated adapters, migrations, references, and
  eval assertions together when their contract changes.
