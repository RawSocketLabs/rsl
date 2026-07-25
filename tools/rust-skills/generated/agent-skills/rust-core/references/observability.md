# Observability and Diagnostics

### CORE-OBS-001 Keep library observability typed and caller-routed

- **Strength:** SHOULD
- **Applies to:** reusable libraries, applications, services, logging,
  structured diagnostics, metrics, operational snapshots, and telemetry
- **Directive:** Define the operational facts and their semantics before choosing
  an ecosystem. Expose only the typed, bounded events, snapshots, counters,
  gauges, or histograms that actual consumers need. Do not invent a universal
  observability facade or backend trait without demonstrated substitution.
- **Why:** A library that picks an ecosystem or installs a global subscriber
  makes that choice for every application linking it, and two such libraries in
  one binary conflict irreconcilably. Typed events leave the routing decision
  with the process that actually owns the telemetry pipeline.
- **Correctness boundary:** Logs and aggregate metrics supplement but never
  replace typed errors, return values, protocol evidence, per-stream
  discontinuities, or other correctness-bearing state. Emitting a diagnostic
  does not handle an error. Observability failure must not silently change the
  operation's domain result unless telemetry is itself the declared operation.
- **Logging recommendation:** Prefer `tracing` when a repository chooses
  structured Rust logging and diagnostics, subject to its dependency, MSRV,
  feature, performance, and target policy. A reusable library may instrument
  with `tracing` directly or through an optional adapter, but must not install a
  global subscriber or exporter. Applications own `tracing-subscriber`,
  filtering, formatting, export, and process-wide initialization. `tracing` is
  recommended, not required; preserve an established `log` or other local
  ecosystem when migration lacks sufficient benefit.
- **Metrics and snapshots:** Do not mandate `metrics`, OpenTelemetry, or another
  exporter across repositories. Let applications adapt the typed core evidence
  to their selected backend. For every instrument define its unit, counter
  monotonicity and reset/wrap/saturation behavior, gauge meaning, histogram
  population and buckets, label set and cardinality bound, aggregation scope,
  concurrency consistency, and export interval where relevant.
- **Volume, cost, and data policy:** Keep labels and fields bounded; do not use
  unbounded user, packet, error-text, or identifier values as metric labels.
  Avoid per-item logging and expensive field construction in hot paths; use
  filtering, sampling, aggregation, or boundary events and measure material
  overhead. Apply repository policy before recording secrets, payloads,
  personal data, or high-volume evidence.
- **Exceptions:** An application may standardize `tracing`, a metrics recorder,
  or OpenTelemetry locally and configure its global process boundary. A
  framework integration may require its own facade. `no_std`, embedded, FFI,
  and externally constrained libraries may expose compact callbacks, counters,
  or snapshots instead. A second proven backend may justify a narrow adapter
  trait.
- **Mechanical owner:** Disabled and enabled instrumentation tests; operation
  equivalence without a subscriber; feature-matrix and MSRV checks; typed
  snapshot and counter-semantics tests; bounded-cardinality review; subscriber
  initialization tests in applications; sensitive-field review; and
  instrumentation-overhead measurements for hot paths.
- **Sources:** Preferences R107, R108, R176, and R194; `tracing` 0.1.44 and
  `tracing-subscriber` 0.3.23 documentation reviewed 2026-07-24.
