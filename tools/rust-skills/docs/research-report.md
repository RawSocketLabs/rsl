# Rust Engineering Skills Research Report

Status: research complete; architecture intentionally deferred

Research snapshot: 2026-07-18

Companion record: [Rust Engineering Preference Record](preference-record.md)

## Executive findings

The proposed system should not be a large Rust style guide copied into every
agent prompt. The strongest evidence supports a layered system:

1. Reusable skills should contain the engineering judgment an agent is unlikely
   to apply consistently without help: how to reason about correctness,
   ownership, protocol validity, streaming state, unsafe boundaries, performance
   evidence, and review risk.
2. Repository instructions should contain facts and choices that are true only
   for that repository: commands, MSRV, adopted profiles, first-class targets,
   dependency policy, protocol authorities, overload behavior, and local
   exceptions.
3. Mechanical policy should live in tools such as rustfmt, Cargo workspace
   lints, Clippy, cargo-deny, tests, fuzz targets, benchmarks, and CI. Agents
   should run and interpret those tools, not restate their full rule sets.
4. The portable authoring format should stay within the common Agent Skills
   subset. Product-specific metadata and instruction files require generated,
   validated adapters because discovery, precedence, and duplicate-name behavior
   differ materially among Codex, Claude Code, Cursor, and Zed.
5. The initial skills need evals against a no-skill baseline. A rule belongs in a
   skill only when it improves realistic tasks enough to justify its context and
   maintenance cost.

Two owner repositories provide stronger evidence than generic guidance:

- `rsl-deps` is already a deliberate, feature-gated facade for blessed external
  dependencies. The standards should preserve its zero-default-feature,
  external-only, registry-only contract and require discussion for additions.
- `bitsandbytes` already defines the owner's protocol vocabulary and behavior.
  Its permissive decode, required-by-default builder, explicit bit/byte order,
  verbatim-versus-canonical encoding, bounded buffering, and adversarial tests
  should override a generic protocol template where the crate is adopted.

This report identifies constraints and source material. It does **not** finalize
skill boundaries, repository schemas, adapter layout, or enforcement policy;
those are Stage 2 architecture decisions.

## Method

The review used four tests for each candidate source:

- **Authority:** Is the source primary, official, or grounded in maintained
  production code?
- **Fit:** Does it match the priorities and repository profiles in the preference
  record?
- **Enforceability:** Is the advice agent judgment, repository context, or a
  mechanical check?
- **Reuse safety:** Is the license clear, and can the useful idea be adapted
  without importing incompatible wording or assumptions?

Repository reviews were pinned to a commit where available. Product behavior was
checked against current primary documentation on 2026-07-18 because agent
discovery behavior changes over time.

## Source assessment matrix

| Source | Strongest contribution | Main limitation | Recommended use | License/reuse posture |
|---|---|---|---|---|
| [leonardomso/rust-skills at `fd2a861`](https://github.com/leonardomso/rust-skills/tree/fd2a861ab0406a4ac536a55274d14ea6fd1ca9c9) | Auditable one-rule files, examples and counterexamples, broad topic coverage | Very large catalog, repeated or absolute rules, context-free dependency recommendations, static priorities | Mine topic coverage, examples, and possible eval cases; do not adopt its whole catalog | MIT; attribution and license inventory still required for copied material |
| [actionbook/rust-skills at `fa60f79`](https://github.com/actionbook/rust-skills/tree/fa60f7931223646fb71c4586b4a6c8545016076a) | Domain constraints before mechanical Rust fixes; baseline-versus-skill testing concept | Tool-specific router and negotiation machinery is large and prescriptive; triggers are overly broad | Adapt the domain -> design -> mechanics reasoning order and eval idea | Repository has an MIT badge but no license file in the reviewed revision; treat text as unavailable for copying until clarified |
| [David Barsky's Rust skills gist](https://gist.github.com/davidbarsky/8fae6dc45c294297db582378284bd1f2) | Compact rustdoc guidance and focused trigger descriptions | The style skill contains strong personal absolutes and tool assumptions | Use as a prompt-design comparison; derive rustdoc rules from official Rust sources | No license was visible; do not copy text |
| [HASH Rust skills at `f421c64`](https://github.com/hashintel/hash/tree/f421c643cc9a16288488a4db8a12547050632333/.claude/skills) | Concise entry files plus references; repo-specific errors and documentation practices; scoped exceptions | Tool adapters differ, and several rules are HASH-specific rather than general Rust guidance | Learn from the repo-fact/domain-exception split | Rust skill files declare AGPL-3.0; do not copy wording into a permissively licensed system without a deliberate license decision |
| [Microsoft Pragmatic Rust Guidelines at `95ac4c8`](https://github.com/microsoft/rust-guidelines/tree/95ac4c828fb8034853f332c729207a9fe5ab9cdc) | Stable rule identifiers, rationale, MUST/SHOULD strength, topical checklists, generated agent aggregate | Large-service and COGS assumptions make some recommendations inappropriate as defaults | Cite per topic; adapt identifier/rationale structure and mechanical-check links | MIT |
| [Rust API Guidelines at `97a0969`](https://github.com/rust-lang/api-guidelines/tree/97a0969cb07fe4cabb0eed8a56234053f47d83dc) | Public API vocabulary and checklists for traits, conversions, builders, type safety, and documentation | API-only scope and some dated examples; not a systems, DSP, or protocol standard | Primary reference for public library API reviews, filtered by repository profile | MIT OR Apache-2.0 |
| [Azure SDK for Rust root instructions at `b7e1b86`](https://github.com/Azure/azure-sdk-for-rust/blob/b7e1b86714dfd6dbe3e32d0deef32fba6d48098c/AGENTS.md) | Real nested repository instructions, generated-code boundaries, exact commands, local package facts, semver propagation | Root file is long and mixes generic style with repo facts | Model for adoption records and nested local instructions, not for a global Rust skill | MIT |
| [ANSSI Rust secure-coding guide at `3f9e2e2`](https://github.com/ANSSI-FR/rust-guide/tree/3f9e2e28095d0eeb35ed61e212a82be12fde4884) | Threat-model framing, unsafe containment, dependency and FFI checklists, normative labels | Security-critical scope is narrower than ordinary Rust development; some sections are incomplete | Source for security-sensitive profiles and review prompts | Open Licence 2.0; paraphrase and track attribution unless compatibility is reviewed |
| [Official Rust Style Guide](https://doc.rust-lang.org/style-guide/) | Canonical formatting baseline and relationship to rustfmt | Formatting is not an agent judgment framework | Configure rustfmt and avoid duplicating formatting rules in skills | Link to the official source; do not fork its content unnecessarily |
| [Clippy usage](https://doc.rust-lang.org/clippy/usage.html), [configuration](https://doc.rust-lang.org/clippy/configuration.html), and [lint list](https://rust-lang.github.io/rust-clippy/master/index.html) | Mechanical lint policy; documented distinction among default, pedantic, nursery, and restriction lints | Group-wide restriction or nursery adoption is noisy and may contain conflicting lints | Workspace lint configuration with curated, individually justified additions and scoped allows | Link to official sources; generated lint inventory belongs in tooling |
| [Rustonomicon](https://doc.rust-lang.org/nomicon/) | Unsafe design concepts, privacy boundaries, safe wrappers, and FFI hazards | It describes itself as incomplete and may lag the language | Conceptual unsafe review source; verify operational details against current authoritative material | Official Rust documentation |
| [Unsafe Code Guidelines Reference](https://rust-lang.github.io/unsafe-code-guidelines/) | Useful glossary and history | The project states that most of the reference is abandoned and recommendations may change | Do not use as a normative source; prefer the Rust Reference and current `t-opsem` consensus | Link only as a status warning |
| [Agent Skills specification](https://agentskills.io/specification) | Portable `SKILL.md` schema and progressive-disclosure contract | Individual products extend it and disagree about discovery and precedence | Canonical interchange subset and validation target | Specification permits an explicit per-skill license field; the system still needs its own top-level license |

## Owner repository findings

### RawSocketLabs `rsl`

The reviewed repository was [RawSocketLabs/rsl at
`ae83b803`](https://github.com/RawSocketLabs/rsl/tree/ae83b80307a4941ec88e84bbb91e444977923885).
It is a better source for owner-specific conventions than external generic Rust
guides.

The [root `AGENTS.md`](https://github.com/RawSocketLabs/rsl/blob/ae83b80307a4941ec88e84bbb91e444977923885/AGENTS.md)
already demonstrates several target practices:

- a workspace map and local instruction files;
- a stated MSRV and workspace lint policy;
- scoped unsafe policy;
- exact verification commands;
- lockstep path-plus-version dependencies;
- release-plz and Conventional Commits expectations; and
- `CLAUDE.md` links to canonical `AGENTS.md` files.

The last point works in the current repository, but generated adapters remain the
safer general default because symlink support and behavior vary by product and
operating environment.

### `rsl-deps`

The reviewed [`rsl-deps` contributor guide](https://github.com/RawSocketLabs/rsl/blob/ae83b80307a4941ec88e84bbb91e444977923885/rsl-deps/AGENTS.md)
defines a specific policy, not merely a convenience crate:

- only external crates belong in it;
- every dependency is optional and default features are empty;
- registry version pins live in one manifest;
- re-exports use canonical crate names behind capability features;
- Git dependencies are forbidden so publication remains possible; and
- additions require manifest, feature, re-export, documentation, and
  cargo-deny updates.

The [`Cargo.toml`](https://github.com/RawSocketLabs/rsl/blob/ae83b80307a4941ec88e84bbb91e444977923885/rsl-deps/Cargo.toml)
shows the intended breadth: errors, tracing, serialization, byte buffers, CLI,
crypto/checksum, complex numbers, FFT, Tokio, futures, NATS, and Rayon are
available as opt-in capabilities.

This supports the user's dependency preference with three qualifications:

1. `rsl-deps` should be the preferred entry point only in repositories that
   explicitly adopt the RSL dependency policy.
2. Adding or changing its capabilities can move features, transitive graphs, and
   MSRV for many consumers, so updates need a blast-radius review rather than a
   mechanical edit.
3. Re-exporting third-party APIs creates coupling to upstream public surfaces.
   The facade's versioning and publication strategy must be settled before the
   standards promise compatibility behavior.

### `bitsandbytes`

The detailed [`bnb` contributor guide](https://github.com/RawSocketLabs/rsl/blob/ae83b80307a4941ec88e84bbb91e444977923885/bitsandbytes/bnb/AGENTS.md)
is already a domain design record. Important established contracts include:

- an owned, integer-backed, bit-aware representation;
- independent and explicit `BitOrder` and `ByteOrder`;
- required-by-default builders with opt-in field defaults;
- permissive decoding, with strict decode guards explicitly requested;
- catch-all enum forms that preserve unknown discriminants;
- `to_bytes`/`bit_encode` as verbatim encoding and
  `to_canonical_bytes`/`canonical_bit_encode` as spec-normalizing encoding;
- the encoding form selected per call rather than stored as hidden message
  state;
- bounded, reusable `BitBuf` paths for allocation-controlled streaming;
- hostile count tests that prevent preallocation from untrusted lengths;
- no-std, MSRV, public-documentation, UI-test, property-test, benchmark, and
  transport-adapter requirements; and
- natural layouts for the common MSB/big-endian network and LSB/little-endian
  DBC conventions, with mixed ordering made deliberate and tested.

These findings refine the draft protocol preferences:

- A universal `ValidationPolicy` type must not be imposed on repositories that
  already use the `bitsandbytes` model. The standard should require an explicit,
  reviewable validity policy while allowing the crate's existing builder,
  `validate`, assertion, verbatim, and canonical surfaces to satisfy it.
- “Encoding must not silently validate” is already represented more clearly as
  an explicit per-call verbatim/canonical choice.
- Exact wire round-tripping, unknown preservation, and canonical normalization
  are separate requirements and should be tested separately.
- General protocol guidance should link to the adopted crate's vocabulary rather
  than invent synonymous bit/byte abstractions.

## Detailed source reviews

### leonardomso/rust-skills

The project is useful as an inventory: one rule per file, category prefixes,
“why” explanations, bad/good examples, and cross-references make individual
claims auditable. It is also a warning about scale. The reviewed revision
contains hundreds of small rules and a large index; many rules repeat generic
knowledge, turn contextual tradeoffs into absolutes, or recommend dependencies
without a repository approval process.

Useful adaptations:

- stable identifiers for rules that survive file reorganization;
- small counterexamples for failure modes an agent commonly produces;
- a coverage checklist for ownership, async, unsafe, FFI, testing, API design,
  performance, and documentation.

Do not adapt:

- global claims that every clone allocates or every repository should choose a
  particular collection, hasher, mocking crate, or optimization;
- a giant always-loaded index;
- category priority tables that ignore repository profiles; or
- installation instructions that copy only a `SKILL.md` while leaving referenced
  files behind.

### actionbook/rust-skills

The three-level reasoning model—domain constraints, design choice, then Rust
mechanics—is valuable. It reduces the common failure mode of satisfying the
borrow checker while missing the protocol, concurrency, or lifetime contract.
Its pressure-scenario evals also reinforce comparison against a no-skill
baseline.

The surrounding plugin is not a suitable foundation for this project. It routes
nearly every Rust mention, forces negotiation behavior, adds generated crate
skills, and depends on tool-specific hooks and inheritance conventions. That
machinery would obscure the user's preference for clarity and simple use. The
unclear repository license also prevents text reuse.

### David Barsky's Rust and rustdoc skills

The rustdoc material is a good example of a narrow, triggerable skill. The style
material demonstrates why personal taste must not be mislabeled as universal
engineering law: it contains categorical preferences about loops, iterators,
shadowing, comments, wildcard patterns, and LSP usage. Those may be valid in one
repository but conflict with a reusable decision framework.

Use the compact trigger design as inspiration. Derive documentation content from
official rustdoc and Rust API guidance rather than copying the unlicensed gist.

### HASH

HASH separates a concise skill entry from references and encodes exceptions such
as a repository-specific error stack. That is the right conceptual split:
general guidance should explain error-boundary judgment, while the repository
declares its chosen error types and exceptions.

The implementation also reveals portability risk: `.claude` and `.codex`
content are not identical, and agent-specific metadata is mixed into some
skills. Generated adapters with drift checks would improve that pattern. The
AGPL declaration means its wording is not appropriate source material for a
permissively licensed output without an explicit licensing decision.

### Microsoft Pragmatic Rust Guidelines

This is the strongest external model for rule metadata. Stable identifiers,
MUST/SHOULD labels, a rationale, and an indication of mechanical checks make a
large body navigable. Its explicit “spirit, not letter” posture is compatible
with the desired escape-hatch model.

Individual recommendations must still be filtered. Guidance such as allocator
choice, hasher choice, prelude avoidance, aggressive capacity shrinking, or flat
crate layout is motivated by Microsoft's large-scale service environment. It
should not silently become the default for embedded-adjacent DSP, public
libraries, protocol tooling, or small applications.

### Rust API Guidelines

This should be the main external checklist for public-library ergonomics. It is
particularly useful for conventional traits, conversions, builders, type safety,
documentation, debuggability, and caller control.

It should remain a reference rather than an imported checklist. The source does
not decide streaming overload behavior, numerical contracts, unsafe proof,
protocol validity, or DSP buffers, and some examples show their age. Evals should
test whether agents apply the relevant API principles without overengineering
internal or prototype code.

### Azure SDK for Rust

Azure's nested instruction files demonstrate that local repository facts are
more useful than generic prose: generated-code boundaries, package-specific
commands, crate graphs, service model choices, and semver propagation are stated
near the code they govern.

Its root file is too large to serve as the target template and includes generic
Rust preferences that belong in reusable guidance or tooling. The lesson is to
preserve its precision and nesting while reducing always-loaded volume.

### ANSSI

ANSSI provides a valuable security review vocabulary: distinguish rules from
recommendations, contain unsafe code, review dependency provenance, avoid
attacker-triggerable panics, and harden FFI boundaries. It is most applicable
when a repository declares a hostile-input or security-sensitive profile.

Applying the full guide to every application would raise cost without matching
the user's priority tiers. Security-sensitive rules should therefore be
activated by trust boundary and risk, with a small correctness baseline shared
everywhere.

## Official Rust guidance and mechanical enforcement

### Formatting

The Style Guide defines default formatting and treats rustfmt as the executable
reference. Therefore:

- skills should say to use the repository's stable rustfmt configuration;
- the repository should version `rustfmt.toml` only when it differs from stable
  defaults; and
- detailed whitespace and layout rules should not consume skill context.

### Linting

Clippy's own documentation supports a curated policy:

- `clippy::all` is the normal baseline;
- `pedantic` is intentionally opinionated and needs selected allows;
- `nursery` changes as lints mature; and
- the whole `restriction` group should not be enabled because restriction lints
  can conflict and are meant for individual selection.

Workspace `[lints]` should be the source of truth for levels where supported.
Scoped `#[allow]` annotations need a short reason. A skill should review whether
an exception hides a real bug, but should not replicate the lint catalog.

### Unsafe Rust

The Nomicon is useful for reasoning about unsafe boundaries, but both it and the
Unsafe Code Guidelines site warn against treating their text as a complete,
current operational specification. The system should:

- default to denying unsafe code;
- put necessary unsafe operations behind the smallest safe abstraction;
- require explicit invariants and a safety rationale;
- verify applicable code with Miri, sanitizers, fuzzing, and platform tests; and
- consult the current Rust Reference and current operational-semantics decisions
  when a detail is material.

The unsafe skill content should be a review procedure, not a frozen copy of
unstable aliasing or validity rules.

### Enforcement ownership

| Concern | Canonical enforcement location | Agent responsibility |
|---|---|---|
| Formatting | stable rustfmt and CI diff check | Run the repo command; do not hand-format around the tool |
| Lint levels | workspace Cargo lints and scoped attributes | Interpret findings; challenge broad or unexplained exceptions |
| Licenses, advisories, bans, sources | cargo-deny configuration and CI | Discuss policy changes; never auto-allow a failure |
| MSRV | manifest/rust-version, pinned toolchain job, repo adoption record | Avoid unsupported syntax; run the declared job when relevant |
| Public docs | rustdoc, doctests, `missing_docs`, docs CI | Review concepts, examples, Errors/Panics/Safety sections, and vocabulary |
| Protocol correctness | reference vectors, property tests, fuzz targets, corpus metadata | Identify authorities and invalid/unknown/canonical cases before editing |
| DSP correctness | scalar references, golden/captured fixtures, numerical metrics | Select tolerances by domain and check chunk/boundary equivalence |
| Unsafe/FFI | lint gates, Miri, sanitizers, fuzzing, ABI tests | Review invariants, ownership, unwind, aliasing, and safe wrappers |
| Performance | locally reproducible benchmarks/profiles; controlled CI where stable | Establish before/after evidence and record workload/toolchain/hardware |
| Dependency approval | human review plus manifest/lockfile diff policy | Present alternatives and blast radius before adding or expanding a dependency |
| Conventional Commits/semver | commit lint, release tooling, semver checks where applicable | Classify user-visible change and update supporting artifacts |

## Cross-agent discovery and precedence

The products implement the same broad concepts but not the same resolution
rules. A tool-neutral logical precedence policy can guide agents, but adapters
must respect what each product actually loads.

### Current behavior matrix

| Product | Persistent repository instructions | Skill discovery | Duplicate/nesting behavior | Adapter consequence |
|---|---|---|---|---|
| Codex | Global `~/.codex/AGENTS.md` or override, then one instruction file per directory from project root to CWD; later/closer content wins; 32 KiB combined project default | `.agents/skills` from CWD up to repo root; `~/.agents/skills`; admin/system locations | Same-name skills are not merged and can both appear; symlinks are followed | Keep `AGENTS.md` concise, use unique skill names, and test from realistic working directories |
| Claude Code | Managed, user, project, and local `CLAUDE.md`; root-to-CWD content is concatenated, subdirectory files load when accessed; `AGENTS.md` requires an import or symlink | `.claude/skills`, personal, enterprise, plugin, and nested project locations | Enterprise overrides personal, personal overrides project; nested same-name skills receive qualified names | Generate a minimal `CLAUDE.md` importing canonical `AGENTS.md`, plus generated `.claude/skills` copies |
| Cursor | `.cursor/rules`, team/user rules, and root or nested `AGENTS.md`; nested `AGENTS.md` combines with parent and closer content wins | `.agents/skills`, `.cursor/skills`, their user equivalents, and compatibility paths for Claude/Codex; project directories are scanned | Skills roots are recursive; nested project skills are path-scoped; current docs do not define a safe cross-root duplicate merge | Prefer one generated project skill root; do not install the same name through several compatibility paths |
| Zed Agent | Personal `~/.config/zed/AGENTS.md`; project uses the first matching compatibility filename, checking `.rules` before `AGENTS.md`; project conflicts override personal | only `~/.agents/skills` and `<worktree>/.agents/skills` | Skills must be direct children; project same-name skill overrides global; untrusted worktrees do not load project skills | Flat skill layout is required; adoption checks must warn when `.rules` or another earlier compatibility file masks `AGENTS.md` |

Primary product sources:

- Codex [custom instructions with `AGENTS.md`](https://learn.chatgpt.com/docs/agent-configuration/agents-md)
  and [skill authoring](https://learn.chatgpt.com/docs/build-skills)
- Claude Code [`CLAUDE.md` loading](https://code.claude.com/docs/en/memory)
  and [skills](https://code.claude.com/docs/en/skills)
- Cursor [rules](https://cursor.com/docs/rules) and [skills](https://cursor.com/docs/skills)
- Zed [instructions](https://zed.dev/docs/ai/instructions) and
  [skills](https://zed.dev/docs/ai/skills)

### Safe common subset

The [Agent Skills specification](https://agentskills.io/specification) and Zed's
narrow implementation define the conservative portable subset:

- one flat directory per skill;
- a `SKILL.md` whose parent directory matches a globally unique lowercase,
  hyphenated `name`;
- a concise `description` that states both capability and trigger;
- Markdown instructions;
- optional `scripts/`, `references/`, and `assets/` loaded explicitly.

Manual-only versus implicit invocation is a portable design intent but not a
portable field: Claude Code, Cursor, and Zed understand
`disable-model-invocation`, while Codex expresses the equivalent policy in
`agents/openai.yaml`. The canonical source may model the intent, but each adapter
must emit and test the product-specific representation.

Do not put portable semantics in Claude-only dynamic shell injection, tool
allowlists, subagent fields, Cursor path metadata, Codex UI metadata, or other
vendor extensions. An adapter may add such metadata only when it does not change
the canonical engineering rule.

### Logical policy versus product precedence

The user's intended logical policy remains sound:

1. explicit user instruction;
2. closest applicable repository instruction;
3. parent/root repository instruction;
4. repository-declared domain skill;
5. general Rust skill; and
6. general agent defaults.

Lower levels may strengthen an unconstrained decision but must not silently
override a higher-level choice. Because products concatenate or select files
differently, this policy must be stated in the portable content and verified by
adapter evals; file placement alone cannot guarantee identical behavior.

## Skill and instruction size implications

All current sources favor progressive disclosure:

- the Agent Skills specification recommends a `SKILL.md` below 500 lines and
  roughly 5,000 tokens;
- Codex initially loads only catalog metadata and imposes a budget on that
  catalog;
- Claude recommends `CLAUDE.md` files below about 200 lines and keeps an active
  skill in context across turns;
- Cursor recommends focused rules under 500 lines and references rather than
  copied source; and
- Zed requires direct-child skills and recommends detailed content in referenced
  files.

The practical limit should be much smaller than the maximum. A skill entry should
contain its trigger, decision procedure, non-negotiable safety constraints, and
verification loop. Detailed topic rules, examples, source notes, and exception
guidance should be loaded only when the task reaches that decision.

## Evaluation implications

The official [Agent Skills evaluation guide](https://agentskills.io/skill-creation/evaluating-skills)
supports the requested approach:

- begin with two or three realistic prompts per capability;
- vary phrasing and include at least one boundary or ambiguous case;
- run each case in a clean context with and without the skill, or against the
  previous skill version;
- add objective assertions after inspecting initial outputs;
- use scripts for mechanical assertions and human review for judgment quality;
- capture pass/fail evidence, time, and token cost;
- compare outputs blindly where possible; and
- remove instructions that do not improve results.

Initial Rust evals should stress judgment rather than trivia. Candidate cases:

1. A public library API change where cloning is simple but a reusable-buffer
   design better matches the declared profile.
2. A parser that must preserve an invalid reserved field byte-for-byte while
   also offering canonical encoding.
3. A DSP optimization proposal that introduces SIMD before establishing a
   scalar reference or benchmark.
4. A Tokio application that tries to run sustained CPU-heavy DSP on executor
   workers.
5. A dependency addition already available through `rsl-deps`, with unnecessary
   default features.
6. An unsafe optimization whose safety comment restates operations but does not
   identify invariants.
7. A review task containing adjacent cleanup that should be surfaced but not
   silently included.
8. A conflict between a general Rust preference and a closer repository rule.

Each eval should score at least correctness, scope discipline, evidence quality,
misuse resistance, clarity, and whether the agent found and honored repository
facts. Performance-domain evals should separately measure whether the agent
avoids unsupported claims and proposes locally reproducible evidence.

## Licensing and provenance

The system needs a source ledger before it ships reusable text. For each adopted
idea or example, record:

- source URL and pinned revision;
- source license at that revision;
- whether content was copied, adapted, or independently written;
- attribution or notice obligations; and
- the local rule/eval that uses it.

Recommended policy for the first implementation:

- write rules independently from the preference record and primary sources;
- cite external sources instead of copying long passages;
- permit short adapted examples only from clearly compatible sources with
  attribution;
- quarantine AGPL, unlicensed, or ambiguous-license wording from generated
  outputs; and
- give the skills repository an explicit license before distribution.

No source reviewed here should be copied wholesale. In particular, do not copy
from the unlicensed Actionbook revision or Barsky gist, and do not import HASH's
AGPL skill text into a permissively licensed project without a deliberate legal
decision.

## Preliminary implications for Stage 2

These are constraints for the architecture proposal, not the proposal itself:

- Keep a canonical, tool-neutral source and generate thin product adapters.
- Use globally unique, flat skill names because that is the only layout safe
  across all four products.
- Treat `.agents/skills` as an important common target for Codex, Cursor, and Zed,
  while generating Claude's native `.claude/skills` target.
- Generate a minimal `CLAUDE.md` import for repositories whose canonical facts
  live in `AGENTS.md`.
- Detect competing Zed compatibility files and duplicate skill installations
  instead of guessing which content the user intended.
- Keep repo facts short and local; do not generate a 2,000-line preference record
  into `AGENTS.md`.
- Model rule strength, scope, rationale, evidence, exceptions, mechanical owner,
  and source provenance in the canonical material.
- Let repositories declare profiles and overrides rather than choosing one
  universal balance of performance, ergonomics, compatibility, and security.
- Prefer references organized by decision point over hundreds of isolated
  slogan rules.
- Make adapter generation deterministic, versioned, and drift-checked.
- Test discovery, triggering, precedence, and output quality independently.

## Questions resolved by research

- `rsl-deps` and `bitsandbytes` are in the
  [RawSocketLabs/rsl](https://github.com/RawSocketLabs/rsl) monorepo.
- `rsl-deps` is an external-only, optional, zero-default-feature facade with
  registry pins and canonical re-exports.
- `bitsandbytes` is the relevant protocol vocabulary and already specifies
  permissive decode plus verbatim/canonical encoding behavior.
- Codex, Cursor, and Zed share `.agents/skills` as a native project location.
- Claude Code requires a `.claude` adapter and an explicit import if `AGENTS.md`
  is canonical.
- Flat, unique skill names are necessary for Zed and safest across products.
- Generated adapters need discovery tests because same-name and nested-skill
  behavior is not portable.

## Post-research owner decisions

The owner subsequently confirmed:

1. a rolling twelve-month MSRV window, with exact repository pins and local
   overrides;
2. Apple Silicon as the first-class macOS target, with Intel correctness retained
   when practical but no default Intel optimization requirement;
3. dual MIT OR Apache-2.0 licensing for the standards system;
4. renewed discussion for dependency changes that alter features, graph, MSRV,
   unsafe exposure, or behavior, while routine lockfile-only updates remain in
   the normal repository process;
5. Markdown-first, directly readable canonical Agent Skills rather than an
   initial general rule compiler; and
6. committed generated adapters with reproducible generation and drift checks.

The [Stage 2 architecture proposal](architecture-proposal.md) applies those
decisions. Remaining implementation details include MSRV advancement automation,
the default dependency license allowlist, eval-artifact retention, pilot
selection, and verified Cursor behavior when both `.agents/skills` and
`.claude/skills` contain the same skill.

Testing standards, example standards, and nonmechanical code-style preferences
also require a dedicated owner refinement round before final skill content is
written.

## Research conclusion

The proposed project is justified, but its value will come from calibrated
judgment and repository adoption—not from maximizing the number of rules. The
best external sources are useful as references and structural examples. The
owner's current `rsl-deps` and `bitsandbytes` code provide the most important
domain evidence. The next stage is the owner review of the architecture proposal
and the requested testing, examples, and style refinement. No `rsl-rust-core`,
`rsl-rust-review`, templates, generators, or eval fixtures were created during
research.
