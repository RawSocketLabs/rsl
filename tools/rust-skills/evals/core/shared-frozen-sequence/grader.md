# Grader

## Expected observations

- The declared topology is immutable shared data crossing thread boundaries.
- `Arc<[f32]>` represents that topology and makes logical plan clones share the
  coefficient allocation.
- `Rc<[f32]>` would be appropriate only without the cross-thread requirement;
  `Box<[f32]>` would fit unique frozen ownership; `Vec<f32>` remains appropriate
  while building the sequence.
- Replacing independent `Vec` clones with shared ownership changes semantics and
  introduces reference-count and lifetime tradeoffs.

## Acceptable outcomes

- Store `Arc<[f32]>`, convert the constructor's `Vec<f32>` into it, preserve a
  slice accessor, and test cloning and cross-thread access.
- Retain `Vec<f32>` only if new fixture evidence contradicts the stated
  ownership topology or profiling premise.

## Forbidden behavior

- Use `Rc<[f32]>` despite the cross-thread requirement.
- Use `Arc<Mutex<Vec<f32>>>` when neither mutation nor locking is required.
- Claim lower total memory or faster execution solely from pointer size or
  asymptotic clone cost.
- Claim a benchmark or profiler rerun that was not performed.

## Objective assertions

- The crate compiles and tests pass after an implementation run.
- `FilterPlan::coefficients` preserves its slice behavior.
- Cloned plans can be moved into a scoped thread and read successfully.
- The implementation does not add unsafe code or mutable shared state.

## Scoring

Score 0-2 each for ownership selection, behavior preservation, tradeoff
analysis, test quality, and truthful performance evidence. Passing requires at
least 8/10 and no forbidden behavior.
