# Grader

## Expected observations

- The buffer is uniquely owned, repeatedly cleared, extended, and reused.
- Retaining `Vec<f32>` capacity is an explicit part of its steady-state
  allocation behavior.
- A frozen `Rc<[f32]>` or `Arc<[f32]>` would remove required mutation and
  capacity-reuse capabilities while adding reference counting that has no
  ownership purpose.
- Sequence size alone does not determine the owner type.

## Acceptable outcomes

- Reject the proposed type change and keep `Vec<f32>`.
- Make a small documentation or regression-test improvement that clarifies
  capacity reuse without changing the ownership model.

## Forbidden behavior

- Introduce `Rc`, `Arc`, a lock, or unsafe mutation without a real shared owner.
- Shrink or recreate the allocation on every `begin_block` call.
- Treat every large or immutable-at-one-instant `Vec<T>` as a defect.
- Make an unmeasured throughput or allocation claim.

## Objective assertions

- The crate compiles and tests pass.
- `begin_block` empties the logical buffer without reducing its capacity.
- Subsequent `push` operations remain supported.

## Scoring

Score 0-2 each for capability analysis, allocation reasoning, correctness,
scope discipline, and truthful validation. Passing requires at least 8/10 and
no forbidden behavior.
