# Security Engineering

### SEC-THREAT-001 Start security review from assets and trust boundaries

- **Strength:** MUST
- **Applies to:** security-sensitive input, privileges, secrets, cryptography,
  persistence, network services, plugins, build systems, and supply chain
- **Directive:** Identify assets, adversaries, privileges, entry points, trust
  transitions, abuse cases, failure impact, and repository risk posture before
  prescribing mitigations. Trace authentication, authorization, integrity,
  confidentiality, availability, replay, downgrade, resource exhaustion, and
  audit requirements that actually apply.
- **Why:** A generic security checklist can miss the reachable threat while
  imposing irrelevant complexity.
- **Exceptions:** A narrow dependency-only change may reuse a current reviewed
  threat model when its trust boundaries are unchanged.
- **Mechanical owner:** Threat-model review, abuse tests, dependency and unsafe
  routing, and security-profile validation.
- **Sources:** ANSSI secure Rust guidance and preferences R68-R72.

### SEC-CRYPTO-001 Use reviewed cryptographic constructions and dependencies

- **Strength:** MUST
- **Applies to:** cryptography, authentication, key derivation, signatures,
  randomness, secret storage, and protocol security
- **Directive:** Do not invent cryptographic algorithms or compose primitives
  without an authoritative protocol and qualified review. Pin reviewed crates
  and features through repository dependency policy; define algorithm and
  parameter agility, key lifecycle, randomness, zeroization expectations,
  side-channel scope, interoperability vectors, and failure behavior.
- **Why:** Correct-looking primitive calls can form an insecure construction or
  fail through key, nonce, downgrade, or error handling.
- **Exceptions:** Educational or research experiments must be labeled
  non-production and kept from production trust paths.
- **Mechanical owner:** Known-answer and interoperability tests, dependency and
  feature audit, secret-handling review, and specialist review where required.
- **Sources:** ANSSI secure Rust guidance and repository cryptographic authority.

### SEC-DATA-001 Keep sensitive data out of uncontrolled diagnostics

- **Strength:** MUST
- **Applies to:** logs, tracing fields, metrics labels, snapshots, errors,
  panic messages, fixtures, fuzz corpora, and generated reports
- **Directive:** Classify payloads, secrets, identifiers, and personal data
  before recording them. Prefer typed bounded redacted evidence, keep metric
  labels low-cardinality, and apply retention and access policy to fixtures and
  reports.
- **Why:** Diagnostics and test artifacts often outlive the operation and cross
  trust boundaries unnoticed.
- **Exceptions:** An approved forensic workflow may retain sensitive evidence
  under explicit access, encryption, retention, and deletion controls.
- **Mechanical owner:** Field and fixture review, redaction tests, cardinality
  tests, and repository data policy.
- **Sources:** Preferences R107-R108 and R194.
