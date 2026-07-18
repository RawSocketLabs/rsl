# Dependencies and Change

### CORE-DEP-001 Discuss every material dependency change

- **Strength:** MUST
- **Applies to:** direct, development, benchmark, build, and facade dependencies
- **Directive:** Obtain owner direction before adding or materially changing a
  dependency. Material means expanding features or the resolved graph, raising
  MSRV, changing unsafe exposure, or changing behavior.
- **Exceptions:** A lockfile-only update inside approved constraints follows the
  repository's normal process.
- **Mechanical owner:** Manifest and lockfile review, cargo-deny.
- **Sources:** Preference R68, R70, R122, R136.

### CORE-DEP-002 Prefer an adopted `rsl-deps` capability

- **Strength:** SHOULD
- **Applies to:** repositories that explicitly adopt RSL dependency policy
- **Directive:** Check `rsl-deps` before proposing a normal external dependency.
  Preserve optional features, empty defaults, registry sources, and canonical
  re-exports. Treat a new facade capability as a broad dependency change.
- **Exceptions:** Use a direct dependency when local rules require it or the
  facade cannot express the needed feature/MSRV contract; explain why.
- **Mechanical owner:** Cargo metadata and dependency review.
- **Sources:** Preference R69-R73; RawSocketLabs/rsl `rsl-deps` instructions.

### CORE-DEP-003 Configure features deliberately

- **Strength:** MUST
- **Applies to:** Cargo dependencies and crate features
- **Directive:** Disable unnecessary default features, gate optional integrations,
  test meaningful configurations, and avoid a feature powerset without an
  interaction risk.
- **Exceptions:** Retain upstream defaults when they are the reviewed and intended
  contract.
- **Mechanical owner:** Cargo feature matrix and CI.
- **Sources:** Preference R71, R128.

### CORE-CHANGE-001 Keep task changes scoped

- **Strength:** MUST
- **Applies to:** agent-authored changes
- **Directive:** Implement the requested change and necessary supporting updates.
  Do not silently bundle unrelated cleanup, formatting churn, dependency updates,
  or speculative refactors. Surface worthwhile adjacent work as a choice.
- **Exceptions:** Fix an adjacent issue only when required for correctness or
  verification of the requested result, and report it.
- **Mechanical owner:** Diff review.
- **Sources:** Preference R79-R81.

### CORE-CHANGE-002 Respect version, generation, and ownership boundaries

- **Strength:** MUST
- **Applies to:** generated files, APIs, and pre-1.0 repositories
- **Directive:** Edit canonical sources, regenerate derived files, and use
  Conventional Commits to expose incompatible changes. Preserve exact standards,
  MSRV, and dependency pins declared by the repository.
- **Exceptions:** Breaking changes before 1.0 are allowed when explicit and
  supported artifacts change together.
- **Mechanical owner:** Generation drift, semver/commit checks, CI.
- **Sources:** Preference R5, R74, R81, R116-R124.
