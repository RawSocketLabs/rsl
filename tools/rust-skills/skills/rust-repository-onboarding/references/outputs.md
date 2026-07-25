# Repository Adoption Output

## Default layout

For a repository without established equivalents, generate:

```text
AGENTS.md
.rust-skills/
├── adoption.toml
├── repository-profile.md
├── enabled-skills.md
├── validation.toml
├── validation.md
├── decisions/
│   ├── compatibility.md
│   ├── errors.md
│   ├── performance.md
│   ├── protocol.md
│   ├── testing.md
│   └── unsafe.md
└── unresolved.md
scripts/
└── validate-rust.py
```

Omit irrelevant decision files. In a mature repository, keep existing local
rules canonical and make `.rust-skills/` index them instead of copying them.

## Required provenance

Record schema, standards version or tag, exact source revision, content hashes,
installation mode, adapter family, adoption date, organization layer, one base
profile, capabilities, component overrides, enabled skills, local instruction
paths, migration state, and unresolved decisions. The renderer derives the
generated-manifest SHA-256 and per-skill bundle hashes from the exact selected
adapter; never invent these values in an answers file.

Each component overlay records a repository-relative path, base, capabilities,
enabled subset of the installed skills, and rationale. Reject absolute or parent
paths. The root `enabled_skills` is the installed union; a component may narrow
activation but may not name an uninstalled skill.

## Required policy

Record compatibility and MSRV, first-class targets, public API and errors,
feature policy, dependencies and licenses, unsafe and FFI, performance and
allocation, testing tiers, generated content, documentation, examples, review
format, changelog and release behavior, and exact required and optional
validation.

When applicable, also record:

- protocol authority, terminology, validation, parser budgets, partial and
  malformed consumption, unknown/reserved behavior, evidence, and vectors;
- DSP vocabulary, numeric contract, stream metadata, rate, discontinuity,
  reset/finish, timing, and signal fixtures;
- async runtime and parallel pool ownership, queue budgets, overload,
  cancellation, spawned-work lifecycle, and shutdown;
- embedded targets, allocator, panic, interrupts, linker, flashing, hardware,
  timing, stack, and size policy.

## Validation contract

Each check has a stable ID, purpose, command as an argument array, tier
(`required` or `optional`), applicability, prerequisites, timeout, and expected
working directory. `required_tools` names executables that must exist in
addition to the command's first argument, including Cargo subcommands such as
`cargo-deny` or `cargo-miri`. The dispatcher runs from the repository root,
avoids a shell, rejects working directories outside the root, prints clear
status, emits JSON on request, and never reports an unavailable tool as passed.
Required failures, skips, and unavailable tools make the workflow fail;
explicitly inapplicable checks do not.

## Precedence in the generated `AGENTS.md`

Record:

1. current explicit user instruction;
2. closest directory-specific instruction;
3. repository-wide decisions and mechanical configuration;
4. confirmed component and profile defaults;
5. organization preferences;
6. applicable shared skills;
7. authoritative references;
8. approved guidance;
9. curated examples;
10. advisory or historical material;
11. general model knowledge.

Require agents to surface apparent correctness, soundness, or safety conflicts
instead of applying them silently.
