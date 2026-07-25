---
name: rust-core
description: Apply universal Rust engineering priorities, repository precedence, evidence discipline, and proportional design judgment. Use for material Rust work before selecting task, domain, or technique skills. Do not use alone when a specialized Rust skill owns the affected contract.
---

# Rust Core

## Apply precedence

Apply guidance in this order:

1. explicit user instruction for the current task;
2. closest directory-specific instructions and decisions;
3. repository-wide decisions and mechanical configuration;
4. confirmed component and repository-profile defaults;
5. organization-wide preferences;
6. applicable task and capability skills;
7. official authoritative references;
8. approved ecosystem guidance;
9. curated examples;
10. advisory or historical material;
11. general model knowledge.

A higher layer may override a lower one. Never replace a verified repository
fact with a portable preference. Surface an apparent override of correctness,
soundness, security, or an explicit compatibility guarantee instead of silently
applying it.

## Establish the contract

1. Read the current request and every applicable `AGENTS.md`, adopted profile,
   local decision, manifest, toolchain file, lint configuration, and generated
   boundary.
2. Separate verified repository facts, explicit decisions, organization
   preferences, recommendations, and unresolved questions.
3. Resolve conflicts with the precedence above.
4. Identify the affected behavior, trust boundary, ownership, lifecycle,
   platform, hot path, and evidence before choosing an implementation.

## Route ownership

Activate `$rust-implement` for changes and `$rust-review` for reviews. Route
public APIs to `$rust-api-design`, evidence to `$rust-testing`, and domain or
risk work to `$rust-protocol`, `$rust-dsp`, `$rust-performance`,
`$rust-async-concurrency`, `$rust-unsafe-ffi`, `$rust-dependencies-security`, or
`$rust-embedded`. Do not copy their rules into the current response.

Read [principles](references/principles.md) for repository-profile tradeoffs,
abstraction decisions, and universal priorities. Read
[observability](references/observability.md) when operational events, logging,
metrics, tracing, or telemetry are implicated.

## Preserve universal boundaries

- Prefer the smallest design that preserves the real contract.
- Keep normal use clear and difficult to misuse; make consequential escape
  hatches explicit.
- Do not introduce production panics for ordinary operational failures.
- Deny unsafe by default and route permitted unsafe work to `$rust-unsafe-ffi`.
- Do not claim performance, command success, or platform behavior without
  observed evidence.
- Keep repository facts local; shared skills provide judgment and workflows.

## Output

Report the contract applied, skills activated, exact verification observed,
unavailable or skipped evidence, material tradeoffs, and unresolved risks.
