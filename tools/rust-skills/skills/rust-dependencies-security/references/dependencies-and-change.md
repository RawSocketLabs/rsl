# Dependencies and Change

### CORE-DEP-001 Discuss every material dependency change

- **Strength:** MUST
- **Applies to:** direct, development, benchmark, build, and facade dependencies
- **Directive:** Obtain owner direction before adding or materially changing a
  dependency. Material means expanding features or the resolved graph, raising
  MSRV, changing unsafe exposure, or changing behavior.
- **Why:** A dependency is a standing commitment to someone else's release
  cadence, license, MSRV, unsafe code, and advisory history, and it is far harder
  to remove than to add once callers depend on its types. One line in a manifest
  is a poor place for a decision at that scale to be made silently.
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
- **Why:** Independent direct dependencies drift to different versions of the
  same crate, which multiplies the resolved graph, duplicates types that no
  longer interoperate, and leaves each repository to rediscover the same feature
  and MSRV constraints. The facade holds that decision in one reviewed place.
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
- **Why:** Default features are chosen for the average consumer, not this one, so
  they routinely pull in `std`, a runtime, or a serialization stack the
  repository never wanted — and because features are additive across the graph,
  one crate enabling a default re-enables it for everyone.
- **Exceptions:** Retain upstream defaults when they are the reviewed and intended
  contract.
- **Mechanical owner:** Cargo feature matrix and CI.
- **Sources:** Preference R71, R128.

### CORE-DEP-004 Name features for their truthful public contract

- **Strength:** SHOULD
- **Applies to:** public Cargo features for optional capabilities and ecosystem
  integrations
- **Directive:** Use a capability name such as `async` or `parallel` only when
  enabling it leaves the public API, required runtime or pool, observable
  semantics, and compatibility promise genuinely ecosystem-neutral. Use the
  ecosystem name such as `tokio`, `rayon`, or `serde` when the feature exposes
  that ecosystem's types or traits, requires its runtime or pool, or otherwise
  commits callers to it. Name the contract, not an aspiration to replace the
  implementation later.
- **Why:** A feature name is the only thing most consumers read before enabling
  it. Calling a Tokio-bound integration `async` promises neutrality the code does
  not deliver, and the caller discovers the real commitment when their runtime
  panics — by which point the name is public API and expensive to correct.
- **Manifest design:** Keep features positive and additive; avoid placeholder
  names such as `use-*` and `with-*`. Use `dep:dependency` when an optional
  dependency is an implementation detail and its implicit same-named feature
  should not become public. Document the API, dependencies, runtime assumptions,
  defaults, and interactions enabled by every public feature.
- **Compatibility:** Treat feature names and their promised behavior as public
  API. Adding a feature is usually compatible; removing, renaming, or moving
  existing public behavior behind one is normally breaking. Preserve an old
  feature as a documented forwarding alias during a staged migration when the
  repository's compatibility policy requires it.
- **Validation:** Compile and test each meaningful feature independently, the
  default and no-default configurations, and interaction-prone combinations.
  Inspect the resolved graph with `cargo tree -e features` when dependency
  activation matters; do not rely on `--all-features` alone.
- **Exceptions:** An unpublished application or an explicitly experimental
  feature may follow local naming and compatibility policy. A capability feature
  may use a private implementation dependency only when callers do not inherit
  that dependency's API, runtime ownership, or observable semantics.
- **Mechanical owner:** Cargo manifests, public feature documentation,
  `cargo tree -e features`, feature-matrix checks, and semantic-version review.
- **Sources:** Preferences R24, R71, R128, and R185; Cargo feature and SemVer
  documentation; Rust API Guidelines C-FEATURE.
