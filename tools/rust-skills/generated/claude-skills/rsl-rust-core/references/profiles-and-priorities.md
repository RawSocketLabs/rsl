# Profiles and Priorities

Use one default profile, then apply explicit component and repository overrides.
Security sensitivity, hostile input, unsafe, FFI, and hot paths are local risks,
not whole-repository profiles.

| Profile | Default emphasis |
|---|---|
| `public-library` | Misuse resistance, robust docs, conventional APIs, few entry points, typed errors |
| `internal-library` | Domain precision, focused implementations, expert controls, measured efficiency |
| `performance-application` | Readable business flow plus bounded memory, overload policy, reproducible performance |
| `pragmatic-application` | Clear business logic, maintainability, delivery, targeted optimization |
| `prototype` | Flexibility and learning while preserving memory safety and hostile-input boundaries |

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
- **Exceptions:** Continue without interruption when repository facts clearly
  select the design and the change remains within the requested scope.
- **Mechanical owner:** Human/agent review.
- **Sources:** Preference R112.

### CORE-DESIGN-002 Require abstractions to clarify a real boundary

- **Strength:** SHOULD
- **Applies to:** all profiles
- **Directive:** Add an abstraction when it names a domain concept, contains an
  invariant, removes meaningful duplication, or enables a required substitution.
- **Exceptions:** A performance specialization may be concrete and narrow when
  measurements justify it.
- **Mechanical owner:** Evals and review.
- **Sources:** Preference R8, R9, R10.

### CORE-DESIGN-003 Keep optional execution models caller-owned

- **Strength:** MUST
- **Applies to:** reusable libraries
- **Directive:** Keep the base API synchronous. Gate Tokio and Rayon integration
  behind explicit features and avoid hidden runtime or global-pool ownership.
- **Exceptions:** A repository-specific application may own and require one
  runtime.
- **Mechanical owner:** Cargo features, dependency review, tests.
- **Sources:** Preference R28, R29, R109, R110.
