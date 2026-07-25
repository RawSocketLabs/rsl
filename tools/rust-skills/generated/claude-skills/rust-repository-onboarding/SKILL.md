---
name: rust-repository-onboarding
description: Inspect, interview, profile, configure, and install the modular Rust engineering skills into a repository. Use when adopting or refreshing these skills, generating repository-specific Rust rules, or creating validation workflows. Never finalize policy before repository inspection and user approval.
---

# Onboard a Rust Repository

## Preserve the approval gate

Do not copy skills and stop. Do not write repository policy before inspection,
an adaptive interview, a proposed profile and convention summary, and user
approval. Steps 1-4 complete before anything is written.

## Inspect and propose

1. Read `$rust-core`, all existing instructions, and
   [the inspection contract](references/inspection.md). Run
   `scripts/inspect_repository.py` from the target root when available.
2. Classify the repository and propose exactly one base plus applicable
   capabilities. Mixed workspaces may add confirmed component overlays.
3. Ask the questions in [the adaptive interview](references/interview.md) in
   coherent rounds. Skip facts already proven; ask follow-ups when answers
   conflict with source or materially change the design.
4. Separate facts, user decisions, organization preferences, recommendations,
   and unresolved items. Present the complete proposal and obtain approval.
## Generate and install

5. Generate the outputs in [the adoption contract](references/outputs.md),
   preserving mature local rules as canonical rather than duplicating them.
6. Install only the confirmed skill set and adapter family. Record exact
   standards version, source revision, hashes, profiles, components, and
   migration state.
## Verify

7. Generate `scripts/validate-rust.py` from the packaged template, configure
   required and optional checks using
   [the validation catalog](references/validation-catalog.md), run safe
   available checks, and report each as passed, failed, skipped, unavailable,
   or inapplicable.
## Output

8. Present the generated diff, accept corrections, obtain final confirmation,
   and record deliberately deferred decisions.

## Boundaries

Never overwrite existing instructions, generated files, or adapter directories
without explicit approval. Never claim a tool passed when it was absent. Local
decisions override profiles and external references except that a serious
correctness or safety conflict must be surfaced.
