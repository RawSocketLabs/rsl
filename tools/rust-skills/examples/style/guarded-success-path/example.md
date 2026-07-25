# Flatten Preconditions Without Deleting Durable Rationale

## Before

```rust
fn admit(job: Job, queue: &mut VecDeque<Job>) -> Result<(), AdmitError> {
    // This process-memory bound is intentionally lower than the wire limit.
    if job.payload_bytes() > 0 {
        if queue.len() < MAX_PENDING_JOBS {
            queue.push_back(job);
            Ok(())
        } else {
            Err(AdmitError::QueueFull)
        }
    } else {
        Err(AdmitError::EmptyPayload)
    }
}
```

The valid path is nested beneath independent rejection conditions. The comment
records why the queue bound differs from another plausible limit; neither a
type nor a predicate name preserves that policy decision.

## Review

Invert the independent preconditions so the successful mutation remains at the
function's base indentation. Keep the rationale adjacent to the bound it
explains. Do not extract one-line helpers merely to hit an indentation or
function-length target.

## After

```rust
fn admit(job: Job, queue: &mut VecDeque<Job>) -> Result<(), AdmitError> {
    if job.payload_bytes() == 0 {
        return Err(AdmitError::EmptyPayload);
    }

    // This process-memory bound is intentionally lower than the wire limit.
    if queue.len() >= MAX_PENDING_JOBS {
        return Err(AdmitError::QueueFull);
    }

    queue.push_back(job);
    Ok(())
}
```

## Tests

Test an empty job, a full queue, and successful admission. Assert both the error
variant and that rejected jobs do not mutate the queue. The refactor must
preserve the existing API and behavior.

## Lesson

Use guard clauses when independent preconditions otherwise surround the
successful path. Comments that narrate the branch are unnecessary; comments
that preserve a policy distinction, invariant, authority, or rationale remain
valuable and must move with the code they govern.

## Applies when

- Rejection conditions are independent and can return locally.
- Flattening reduces the state a reader must retain.
- A comment records durable context that names and types cannot express.

## Does not apply when

- One cohesive `match` communicates a state transition or exhaustive domain.
- Symmetric branches are clearer together.
- Extraction would scatter a short sequential operation across trivial helpers.
- The comment merely repeats syntax or no longer describes the implementation.

## Source and provenance

This is an independently written Rust example. CodeAesthetic's nesting and
comment videos supplied design pressures; preferences R163 and R202 define the
qualified local policy, including the explicit rejection of a blanket
no-comments rule.

## Validation

Compile the surrounding crate and run behavior tests for every branch. Review
the final diff for API changes, altered mutation on error, and lost rationale.
