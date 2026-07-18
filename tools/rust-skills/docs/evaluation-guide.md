# Evaluation Guide

Eval cases keep task context separate from expected observations. Never expose a
grader to the agent performing the task.

Each case contains:

- `case.toml`: stable identity, eval class, selected skill, profile, and artifact
  paths;
- `prompt.md`: the exact clean-context user task;
- `fixture/`: the smallest repository or diff needed by the task; and
- `grader.md`: expected observations, acceptable outcomes, forbidden behavior,
  objective assertions, and a scoring rubric.

Run a case in a fresh context without a skill before tuning the skill. Run the
same prompt and unchanged fixture with the selected skill, blind the outputs when
possible, and record agent/product version, elapsed time, token/tool cost,
commands, and output paths outside the task context. Never reconstruct a baseline
from an agent that has seen the grader or preference record.

The initial repository intentionally commits fixtures and graders, not invented
run results. Independent forward runs require a clean agent context and are a
separate validation action.
