# Core Principles

Use one semantic base, then add applicable capability defaults and explicit
component or repository overrides. Profiles propose defaults; they do not
override proven local facts or decisions.

| Base | Default emphasis |
|---|---|
| `public-library` | Misuse resistance, robust docs, conventional APIs, few entry points, typed errors |
| `internal-library` | Domain precision, focused implementations, expert controls, measured efficiency |
| `application` | Clear behavior, maintainability, delivery, operational errors, and targeted optimization |
| `service` | Lifecycle, observability, bounded resources, overload, compatibility, and operations |
| `experimental` | Flexibility and learning while preserving memory safety and explicit trust boundaries |

Capabilities add domain, execution, environment, risk, or structure defaults,
including protocol, parser/serializer, DSP, networking, cryptography, async,
concurrent, real-time, embedded, no-std, FFI, platform-specific,
performance-sensitive, security-sensitive, safety-critical, workspace,
public-API, and generated-code. A mixed workspace may use confirmed component
overlays. Do not infer every risk from a whole-repository label.

Priority tiers are:

1. Correctness, performance, abstraction quality, clarity.
2. Maintainability, simplicity, velocity, security.
3. Compile time, binary size, API stability.

Clarity and simple correct use break a tie with a harder-to-understand
abstraction or optimization. Performance remains top tier, but complexity must
earn its cost with evidence.

### CORE-DESIGN-001 Make consequential decisions proportional

- **Strength:** SHOULD
- **Applies to:** all profiles
- **Directive:** Make a conservative, confined choice when it is reversible. Ask
  before a broad or difficult-to-reverse choice changes architecture or policy.
- **Why:** The cost of asking is one interruption; the cost of guessing scales
  with how hard the choice is to undo. Treating both kinds of decision the same
  way either stalls reversible work or commits the repository to an architecture
  nobody agreed to.
- **Exceptions:** Continue without interruption when repository facts clearly
  select the design and the change remains within the requested scope.
- **Mechanical owner:** Human/agent review.
- **Sources:** Preference R112.

### CORE-DESIGN-002 Require abstractions to clarify a real boundary

- **Strength:** SHOULD
- **Applies to:** all profiles
- **Directive:** Add an abstraction when it names a domain concept, contains an
  invariant, removes meaningful duplication, or separates a decision that must
  vary independently from its use. Account for the coupling introduced by the
  shared contract, accepted inputs, implementors, and evolution policy.
- **Why:** An abstraction is a commitment every implementor and caller must keep
  evolving together. Introduced without a boundary that genuinely varies, it buys
  nothing and charges that coupling forever, while the duplication it replaced
  could have been removed at any time.
- **Exceptions:** A performance specialization may be concrete and narrow when
  measurements justify it. Small duplication can be cheaper than coupling
  unrelated concepts to a premature common interface.
- **Mechanical owner:** Evals and review.
- **Sources:** Preferences R3, R8-R10, and R205; CodeAesthetic advisory source.

### CORE-DESIGN-004 Make dependencies visible without manufacturing interfaces

- **Strength:** SHOULD
- **Applies to:** components that use services, policies, clocks, storage,
  transports, algorithms, or other replaceable behavior
- **Directive:** Pass a required dependency or capability into the component
  when doing so separates construction policy from use, makes ownership
  explicit, or enables demonstrated configuration, substitution, or testing.
  Choose a concrete value, generic parameter, enum, closure, or trait from the
  real variability and lifetime contract.
- **Why:** A component that constructs its own clock, transport, or storage
  cannot be exercised without them, so tests reach for the network and the
  substitution the design implied is unavailable. Manufacturing an interface per
  dependency to fix that trades one untestable design for an unreadable one.
- **Exceptions:** Do not create a framework, factory, trait object, or one trait
  per dependency solely to make mocking convenient. Direct construction is
  reasonable for an inseparable implementation detail with no useful
  substitution boundary. Repository frameworks and compatibility constraints
  may determine the wiring shape.
- **Mechanical owner:** API and implementation review, behavior tests, and
  representative consumer construction.
- **Sources:** Preference R204; qualified CodeAesthetic advisory source.

### CORE-STYLE-006 Keep visibility and exceptions narrow

- **Strength:** SHOULD
- **Applies to:** imports, visibility, lint attributes, and unsafe blocks
- **Directive:** Prefer explicit imports and private items. Use the narrowest
  visibility and lint exception with a reason; prefer checked expectations where
  supported. Keep unsafe blocks minimal with an adjacent `SAFETY` proof.
- **Why:** Visibility and lint exceptions are load-bearing scope: a `pub` that
  was convenient becomes a compatibility commitment, and a module-wide `allow`
  silently covers every later addition to that module, including the one the lint
  existed to catch.
- **Exceptions:** Deliberate preludes, generated code, and conditional compilation
  may require broader scoped policy.
- **Mechanical owner:** Clippy, rustc lints, review.
- **Sources:** Preference R158, R165, R166.
