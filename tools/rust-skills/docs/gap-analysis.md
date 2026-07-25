# Modular Rust Skills Gap Analysis

## Closed structural gaps

- Portable engineering guidance is separated from RSL decisions and
  repository-local conventions.
- Task orchestrators route to one owning domain skill instead of duplicating
  rules.
- Repository profiles compose a semantic base with capabilities and component
  overlays.
- Onboarding inspects, interviews, proposes, waits for approval, generates
  concise local outputs, installs selected skills, and records deferrals and
  provenance.
- References have explicit trust tiers and curated repository sources name only
  the reviewed aspect and revision.
- Examples have a validated transformation format with applicability limits.
- Repository validation distinguishes required and optional checks and never
  treats an unavailable tool as passed.
- Evaluation metadata supports expected skill selection, commands, hidden-test
  interfaces, hard gates, forbidden regressions, graders, and external
  baseline/current results.

## Closed technical coverage gaps

The canonical suite now covers idiomatic implementation and review; public APIs,
ownership, errors, documentation, and SemVer; testing, property testing, and
fuzzing; binary protocols and stateful framing; DSP and streaming continuity;
measured performance, allocation, SIMD, and binary size; async cancellation,
concurrency, queues, and shutdown; unsafe proof and FFI; dependencies, supply
chain, and security; and embedded/no-std work.

## Intentionally combined topics

- Errors and documentation remain in API design because their activation and
  compatibility consequences overlap strongly.
- Property testing and fuzzing remain in testing; protocol and unsafe skills
  route there with domain-specific properties.
- Async and concurrency remain one lifecycle owner.
- Unsafe and FFI remain together because safe boundary proofs dominate both.
- Dependency and security guidance remain together for supply-chain and feature
  review, while application threat modeling is a focused reference.

Split one of these only after real tasks demonstrate independent activation,
ownership, and evaluation needs.

## Remaining product work

1. Run blinded no-skill and current-skill evaluation campaigns and store results
   in an external result system. The repository deliberately does not invent
   those results.
2. Grow from 22 toward at least 30 cases using reduced real-world failures,
   especially additional unsafe, protocol-state, security, and negative-
   activation cases.
3. Onboard bitsandbytes as a controlled pilot and libsdr as a preserve-and-index
   pilot. Their repository changes are separate approval-gated tasks.
4. Verify multi-agent adapter coexistence before enabling simultaneous common
   and Claude installation.
5. Tag a release only after pilot feedback and comparative eval evidence.

These are rollout and evidence gaps, not missing canonical skill domains.
