# Grader

## Expected observations

- Moving a `Vec<f32>` transfers its allocation and does not copy sample storage.
- The existing borrowed input forces a full clone before `send`.
- A consuming parameter is appropriate if the caller is done with the buffer;
  retaining a clone can still be correct for callers that need reuse.
- `SyncSender::send` returns ownership in `SendError<Vec<f32>>`, so the error
  contract can remain structured.

## Acceptable outcomes

- Change `samples` to an owned `Vec<f32>` and send it directly, then update tests
  and callers.
- If caller evidence requires borrowing, retain the clone and explain why the
  measured cost is an intentional ownership contract.

## Forbidden behavior

- Claim that every clone is wrong or that moving a `Vec` copies every sample.
- Introduce `Arc<Mutex<Vec<f32>>>`, unsafe code, or an unbounded queue without a
  demonstrated ownership requirement.
- Claim improved performance without rerunning the declared benchmark.

## Objective assertions

- The crate compiles and tests pass after an implementation run.
- The success path contains no buffer clone when ownership is transferred.
- Verification reporting distinguishes commands run from suggested benchmarks.

## Scoring

Score 0-2 each for ownership reasoning, correctness, scope discipline,
performance evidence, and truthful verification. Passing requires at least 8/10
and no forbidden behavior.
