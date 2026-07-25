---
name: rust-skill-maintenance
description: Maintain this modular Rust skills system, including canonical content, manifests, references, profiles, generated adapters, migrations, examples, evaluations, releases, and compatibility. Use when changing the skills repository itself. Do not hand-edit generated adapters.
---

# Maintain Rust Skills

## Preserve canonical ownership

1. Read `$rust-core`, repository authoring instructions, architecture, source
   ledger, affected schemas, migration map, and existing evals.
2. Read [maintenance workflow](references/maintenance.md).
3. Identify the stable skill and rule owner before editing. Route a rule instead
   of duplicating it.

## Change the system

Author portable runtime content under `skills/`, organization preferences under
`organizations/`, repository choices in adoption output, external evidence in
the reference catalog, transformations in `examples/`, and behavior gates in
`evals/`.
Keep entry files concise and references selectively loaded.

Record the source, trust tier, revision, license posture, limitations, review
date, and refresh rule for external guidance. Preserve stable IDs. Add migration
notes before a rename, split, behavior change, schema change, or generated
layout change.

Never edit `generated/` directly. Regenerate it from the exact canonical source
state. Keep eval prompts isolated from graders and compare unchanged tasks
against a clean baseline or prior release before claiming improvement.

## Validate and release

Run `cargo xtask validate`, `cargo xtask generate --check`, formatting, Clippy,
tests, script smoke tests, and applicable eval validation. Update installation,
maintenance, task maps, provenance, and release metadata together.

## Output

Provide source-to-destination migration detail, retained behavior, behavior
changes, compatibility impact, generated state, validation, eval evidence, and
release implications.
