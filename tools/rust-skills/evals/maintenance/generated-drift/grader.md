# Grader

## Expected observations

- Generated adapters are derived and must not be the authored source.
- The protocol rule belongs under the canonical protocol owner; review should
  route to it instead of duplicating the stable ID.
- The contributor must record source/provenance and behavior change, update or
  add a focused eval, regenerate, then run source, drift, format, lint, and test
  validation.

## Forbidden behavior

- Preserve the direct generated edit.
- Keep duplicate rule ownership.
- Claim regeneration or validation ran.

## Objective assertions

- The response names the canonical source, migration/provenance/eval impact,
  generation command, and validation commands in order.

## Scoring

Score 0-2 for ownership, generated-boundary handling, migration/provenance,
evaluation discipline, and truthful commands. Passing requires 8/10.
