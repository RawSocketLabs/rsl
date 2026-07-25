# Authoring Conventions

## Canonical ownership

- Author portable runtime content only under `skills/`.
- Treat `generated/` as deterministic output; change canonical sources and run
  `cargo xtask generate` instead of editing generated files.
- Put repository facts, commands, targets, and exceptions in repository-local
  `AGENTS.md`, not in a global skill.
- Put formatting, lint levels, dependency bans, and other deterministic policy in
  tooling when tooling can express it reliably.

## Skill packages

- Use a globally unique lowercase hyphenated directory and matching `name`.
- Register every canonical skill in `catalog/skills.toml` and every compatibility
  router in `catalog/compatibility-skills.toml`, and keep each machine-readable
  `skill.toml` beside its `SKILL.md`. A router carries its own identity, never a
  canonical one, and never joins a pack. A future planned package may use
  `catalog/planned/`, but planned manifests are never installable and must move
  beside `SKILL.md` when implemented.
- Declare `requires`, `routes_to`, and `related` separately. `requires` transfers
  instruction ownership and must remain acyclic; `routes_to` is conditional
  activation; `related` is navigation only.
- Name another skill with the `$` activation sigil — `$rust-testing`. Every
  sigil in a package's `SKILL.md` or references must resolve to an installable
  skill and appear in that package's `requires`, `routes_to`, or `related`;
  `cargo xtask validate` enforces both. Prefer the sigil over an unlinked prose
  name so routing stays checkable.
- Keep YAML frontmatter to `name` and `description`. Make the description state
  both capability and triggering contexts.
- Write the body as a short imperative workflow. Link every reference directly
  from `SKILL.md` and say when to read it.
- Keep references one level deep. Add a reference only when selective loading
  saves meaningful context or separates a distinct decision surface.
- Do not add READMEs, changelogs, installation guides, or process history inside
  a skill package.
- Ensure each package identifies activation and exclusion, repository
  inspection prerequisites, related owners, selective references, workflow,
  review questions, validation, expected output, common failure modes, and
  material exceptions. These need not be repetitive headings when the package
  states them clearly in its description, workflow, and referenced rules.

## Rules

Use a stable heading such as `CORE-API-001` and record:

- **Strength:** `MUST`, `MUST NOT`, `SHOULD`, `SHOULD NOT`, `PREFER`,
  `CONSIDER`, or `MAY`.
- **Applies to:** the relevant profiles, risks, or artifact types.
- **Directive:** an actionable decision, not a slogan.
- **Why:** the consequence the rule controls.
- **Exceptions:** when a reasonable alternative exists.
- **Mechanical owner:** a tool or `Human/agent review`.
- **Sources:** owner preference IDs and authoritative external links.

`cargo xtask validate` requires all seven fields on every rule block. Keep
**Why:** immediately after **Directive:** and make it state the consequence the
rule controls, not a restatement of the directive — it is what lets a reader
judge whether an exception applies.

A rule ID is a permanent identity, not a location. The `CORE-*` IDs predate the
modular split and stay with their rules wherever those rules now live, so a
prefix records where a rule came from and the owning package is whichever
`skills/<package>/references/` file holds it. Give every new rule an
owner-scoped prefix instead: `API-`, `PROTO-`, `PERF-`, `TEST-`, `UNSAFE-`,
`FFI-`, `EMBED-`, `SEC-`, or `MAINT-`.

Write independently and update [the source ledger](source-ledger.md) when an
external idea materially influences a rule or eval. Never recycle a removed rule
ID for a different meaning.

## Organization decisions and examples

- Put organization-only preferences in an indexed decision beneath
  `organizations/<organization>/decisions/`; do not strengthen the portable
  skill rule to simulate an overlay.
- Give each decision a normative strength, narrow scope, applicability
  conditions, exceptions, and preference-record IDs.
- Register each curated transformation in `examples/index.toml`.
- Use one reviewed source as the machine-readable primary source and record
  additional influences in provenance and the source ledger.
- Include every required `Before`, `Review`, `After`, `Tests`, `Lesson`,
  `Applies when`, and `Does not apply when` section. Pair a positive example
  with an eval counterexample when a slogan could otherwise become a mechanical
  rewrite.

## Change discipline

- Use Conventional Commits. Before `1.0.0`, mark incompatible rule, schema,
  discovery, or generated-layout changes with `!`.
- Change canonical content, supporting docs, eval assertions, and generated views
  together when their contract changes.
- Run `cargo xtask validate` and `cargo xtask generate --check` before review.
- Compare eval results with a no-skill baseline or the previous released skill.
  Do not tune a task prompt with the grader's desired answer.
- Add a compatibility manifest and migration notes before renaming, splitting,
  or retiring a stable skill identity.
- Treat profile, pack, reference, organization, example, and migration schemas
  as versioned contracts. Update their validators in the same patch.
