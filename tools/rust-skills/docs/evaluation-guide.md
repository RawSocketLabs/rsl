# Evaluation Guide

Eval cases keep task context separate from expected observations. Never expose a
grader to the agent performing the task.

Each case contains:

- `case.toml`: schema-checked identity, eval class, primary and expected skills,
  profile, required commands, hard gates, forbidden regressions, and artifact
  or external-result references;
- `prompt.md`: the exact clean-context user task;
- `fixture/`: the smallest repository or diff needed by the task; and
- `grader.md`: expected observations, acceptable outcomes, forbidden behavior,
  objective assertions, and a scoring rubric.

Schema 2 has this contract:

```toml
schema = 2
id = "stable-case-id"
class = "decision"
skill = "rust-protocol"
profile = "public-library"
expected_skills = ["rust-protocol", "rust-testing"]
required_commands = ["cargo test"]
hard_gates = ["compilation", "hidden-tests", "protocol-conformance"]
forbidden_regressions = ["round-trip-only-evidence", "unsupported-validation-claim"]
hidden_tests = "external"
baseline_results = "external"
current_results = "external"
prompt = "prompt.md"
fixture = "fixture"
grader = "grader.md"
```

`none` means the artifact does not apply. `external` means the harness or result
store supplies it without exposing it to the task agent. Any other artifact
value is a path confined to the case directory and must exist. Baseline and
current outputs normally stay outside the task fixture so a task agent cannot
infer expected behavior from prior runs.

Source validation requires at least 20 cases, the core decision/review/
precedence/discovery classes, and expected-skill coverage for every canonical
package. This is a coverage floor, not proof that one case adequately evaluates
a high-risk domain.

Run a case in a fresh context without a skill before tuning the skill. Run the
same prompt and unchanged fixture with the selected skill, blind the outputs when
possible, and record agent/product version, elapsed time, token/tool cost,
commands, and output paths outside the task context. Never reconstruct a baseline
from an agent that has seen the grader or preference record.

For implementation tasks, the harness should record compilation, declared tests,
hidden tests, Clippy, API/ABI or SemVer checks where applicable, protocol vectors,
diff scope, local-convention adherence, invoked commands, and unsupported claims.
For review tasks, use seeded defects and benign temptations to measure finding
precision and recall separately. Skill-selection scoring compares the activated
set with `expected_skills`; extra activation should incur a cost when it adds
irrelevant procedure or context.

Hard gates are objective outcomes that cannot be traded for rubric points.
Forbidden regressions capture case-specific changes or claims that invalidate a
run even when other work is strong. A grader may award partial credit only after
all hard gates and forbidden-regression checks are resolved.

The initial repository intentionally commits fixtures and graders, not invented
run results. Independent forward runs require a clean agent context and are a
separate validation action.

The current 22-case suite includes paired ownership cases: improve a read-only
borrowed optional API but retain a mutable optional slot; share a measured frozen
sequence but retain a uniquely owned reusable `Vec`. Domain cases cover wire
ordering, unsafe boundaries, async cancellation, dependency features, benchmark
validity, mixed-workspace onboarding, embedded target evidence, DSP
discontinuities, generated-source maintenance, and a readability refactor that
must retain durable rationale. These pairs and domain cases evaluate judgment
and exception handling rather than keyword matching.

Grow the suite from real failures. Reduce each incident to the smallest fixture,
add one primary error and one tempting false positive, run a clean baseline, then
add or change a skill only when the case demonstrates reusable value. Before a
stable release, expand toward at least 30 cases with multiple cases for every
high-risk capability and at least one negative-activation case for each
orchestration skill.
