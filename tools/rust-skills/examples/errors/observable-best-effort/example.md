# Make Best-Effort Processing Observable

## Before

```rust
let records = lines.filter_map(|line| parse(line).ok()).collect::<Vec<_>>();
```

Failures disappear without a declared policy.

## Review

Compact iterator syntax is not the issue; silent loss is. Decide whether one
failure aborts the batch or whether best-effort import is a real product
contract with counts, bounded diagnostics, and quarantine behavior.

## After

Return `Result<Vec<Record>, ParseError>` for fail-fast behavior, or return a
named `ImportReport` containing accepted records, rejected count, and bounded
failure summaries for an approved best-effort workflow.

## Tests

Test all-success, first failure, mixed input, bounded summaries, large failure
counts, and sensitive-data redaction. Verify the caller handles or deliberately
discards the report.

## Lesson

Do not turn `Result` into absence silently. Error loss needs a name, consequence,
observability, bounds, and tests.

## Applies when

- Iterator combinators erase parse, I/O, task, or delivery failures.
- Callers act differently on fail-fast and best-effort behavior.

## Does not apply when

- The input is semantically optional rather than failed.
- A documented probe or cache miss intentionally maps one bounded failure to
  absence.
