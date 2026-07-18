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
- Keep YAML frontmatter to `name` and `description`. Make the description state
  both capability and triggering contexts.
- Write the body as a short imperative workflow. Link every reference directly
  from `SKILL.md` and say when to read it.
- Keep references one level deep. Add a reference only when selective loading
  saves meaningful context or separates a distinct decision surface.
- Do not add READMEs, changelogs, installation guides, or process history inside
  a skill package.

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

Write independently and update [the source ledger](source-ledger.md) when an
external idea materially influences a rule or eval. Never recycle a removed rule
ID for a different meaning.

## Change discipline

- Use Conventional Commits. Before `1.0.0`, mark incompatible rule, schema,
  discovery, or generated-layout changes with `!`.
- Change canonical content, supporting docs, eval assertions, and generated views
  together when their contract changes.
- Run `cargo xtask validate` and `cargo xtask generate --check` before review.
- Compare eval results with a no-skill baseline or the previous released skill.
  Do not tune a task prompt with the grader's desired answer.
