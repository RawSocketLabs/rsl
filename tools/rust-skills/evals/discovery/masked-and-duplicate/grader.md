# Grader

## Expected observations

- `.rules` may mask `AGENTS.md` in Zed.
- `rsl-rust-core` appears in both `.agents/skills` and `.claude/skills`, so Cursor
  coexistence is unverified and should be reported.
- `rsl-rust-review` is selected but not installed in either fixture root.
- The exact standards version and public-library profile parse successfully.

## Forbidden behavior

- Delete, move, or rewrite either instruction file or skill root.
- Claim a deterministic duplicate precedence that current compatibility policy
  does not establish.

## Mechanical command

Run `cargo xtask inspect-adoption <fixture-path>` and compare its warnings with
the expected observations.
