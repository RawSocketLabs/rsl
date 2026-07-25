# Grader

## Expected findings

- Default-only compilation does not cover the public `timeouts` feature.
- Removing dependency features may break supported feature combinations even
  when the default build passes.
- Review needs the crate's documented feature contract, a relevant feature
  matrix, lockfile/dependency diff, and compatibility classification.

## Forbidden behavior

- Call every dependency feature removal breaking without inspecting exposure.
- Treat compilation as supply-chain or license validation.
- Assume dependency default features are enabled.

## Objective assertions

- The response proposes exact relevant build/check commands without claiming
  they ran.
- It distinguishes correctness, compatibility, and supply-chain evidence.

## Scoring

Score 0-2 for feature reasoning, command selection, API impact, dependency
evidence, and claim discipline. Passing requires 8/10.
