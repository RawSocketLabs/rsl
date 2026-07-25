# Skill Maintenance Workflow

### CORE-CHANGE-002 Respect version, generation, and ownership boundaries

- **Strength:** MUST
- **Applies to:** generated files, APIs, and pre-1.0 repositories
- **Directive:** Edit canonical sources, regenerate derived files, and use
  Conventional Commits to expose incompatible changes. Preserve exact standards,
  MSRV, and dependency pins declared by the repository.
- **Why:** A hand-edit to a generated file is reverted by the next generation
  run, silently and without conflict, so the fix disappears at an unrelated
  moment. A breaking change that the commit message does not disclose reaches
  consumers who pinned specifically to avoid it.
- **Exceptions:** Breaking changes before 1.0 are allowed when explicit and
  supported artifacts change together.
- **Mechanical owner:** Generation drift, semver/commit checks, CI.
- **Sources:** Preference R5, R74, R81, R116-R124.

### MAINT-OWN-001 Keep every rule under one portable owner

- **Strength:** MUST
- **Applies to:** skill creation, rule migration, review routing, organization
  overlays, repository adoption, and compatibility packages
- **Directive:** Assign each portable rule one canonical skill owner. Let task
  and review skills route to that owner instead of copying detailed guidance.
  Keep organization and repository preferences in overlays above the portable
  layer.
- **Why:** Duplicate rules drift, conflict, consume context, and obscure
  precedence.
- **Exceptions:** A concise boundary summary may appear in an orchestrator when
  it links to the owner and does not restate the full contract.
- **Mechanical owner:** Catalog graph validation, global rule-ID uniqueness,
  migration map review, and evals.
- **Sources:** Approved modular architecture and preference R126.

### MAINT-GEN-001 Generate adapters from canonical sources

- **Strength:** MUST
- **Applies to:** Codex, common Agent Skills, Claude, and future product adapters
- **Directive:** Author runtime policy only in canonical skill packages and
  generate product layouts deterministically. Verify hashes and reject stale or
  extra generated files. Do not mix product-specific policy into adapter
  metadata.
- **Why:** Hand-edited adapters create invisible behavioral forks.
- **Exceptions:** A product-only discovery file may be generated from explicit
  canonical metadata when its semantic policy remains identical.
- **Mechanical owner:** `cargo xtask generate --check` and generated manifest
  hashes.
- **Sources:** Agent Skills specification and preferences R123-R132.

### MAINT-EVAL-001 Protect comparative evaluation integrity

- **Strength:** MUST
- **Applies to:** eval prompts, fixtures, graders, hidden tests, baselines,
  current runs, and publication claims
- **Directive:** Keep prompts and fixtures isolated from graders and expected
  answers. Run unchanged tasks in clean contexts against a no-skill baseline or
  prior release and the candidate skill. Record model/product, skill revision,
  tools, elapsed time, cost, commands, outputs, hard gates, and rubric results
  outside the task context.
- **Why:** Leaked answers and reconstructed baselines measure prompt exposure
  rather than transferable skill value.
- **Exceptions:** A transparent unit test of the validator itself may include
  expected parser output when it is not presented as an agent-performance eval.
- **Mechanical owner:** Eval schema validation, clean-run procedure, blinded
  grading where practical, and publication review.
- **Sources:** Preferences R133-R139 and the approved evaluation architecture.
