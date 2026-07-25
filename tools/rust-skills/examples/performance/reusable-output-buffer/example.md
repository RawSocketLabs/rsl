# Reuse Output Capacity on a Measured Hot Path

## Before

```rust
fn decode(input: &[u8]) -> Vec<i16> {
    input.iter().map(|byte| i16::from(*byte)).collect()
}
```

A sustained measured path allocates one output vector per frame.

## Review

Moving a vector is cheap, but constructing it repeatedly allocates. Preserve the
owned-return common path if it is useful; add reuse only for a demonstrated
caller that owns a stable buffer lifecycle. Do not jump to unsafe or SIMD before
measuring the allocation and conversion work.

## After

```rust
fn decode_into(input: &[u8], output: &mut Vec<i16>) {
    output.clear();
    output.reserve(input.len().saturating_sub(output.capacity()));
    output.extend(input.iter().map(|byte| i16::from(*byte)));
}
```

The caller owns recycling and overload policy.

## Tests

Share expected-output tests between owned and reusable paths. Benchmark a size
sweep with representative input and restore buffer state outside the timed
operation. Measure allocations before and after. Add SIMD only behind a correct
fallback and shared equivalence tests.

## Lesson

Buffer reuse is an ownership contract. Add it for a measured sustained path and
keep correctness evidence independent from the optimization.

## Applies when

- Allocation is measured on a repeated path with stable buffer ownership.
- The caller can reuse capacity without retaining stale logical contents.

## Does not apply when

- The path is cold or clarity matters more than an unmeasured allocation.
- Output must outlive the next call or be shared independently.
