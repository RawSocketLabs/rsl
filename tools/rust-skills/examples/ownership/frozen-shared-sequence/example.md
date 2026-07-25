# Choose Ownership for a Frozen Shared Sequence

## Before

```rust
#[derive(Clone)]
pub struct FilterPlan {
    coefficients: Vec<f32>,
}

impl FilterPlan {
    pub fn new(coefficients: Vec<f32>) -> Self {
        Self { coefficients }
    }

    pub fn coefficients(&self) -> &[f32] {
        &self.coefficients
    }
}
```

Suppose profiling and object-lifetime inspection establish that a large plan is
cloned into several long-lived worker configurations, never mutated, and sent
across threads. Each derived `Vec` clone owns an independent element allocation.

## Review

The requirements are frozen shape, intentional shared ownership, and
cross-thread transfer. Those requirements point to `Arc<[f32]>`. If all owners
were confined to one thread, `Rc<[f32]>` would avoid atomic reference counting.
If the plan had one owner and did not need cheap clones, `Box<[f32]>` would state
the frozen unique-ownership contract.

This recommendation depends on evidence. A builder, scratch buffer, growing
collection, independently mutable snapshot, or capacity-reuse path should
retain `Vec<f32>`. Replacing deep copies with reference counting also changes
ownership semantics, so tests must confirm that sharing is intended.

## After

```rust
use std::sync::Arc;

#[derive(Clone)]
pub struct FilterPlan {
    coefficients: Arc<[f32]>,
}

impl FilterPlan {
    pub fn new(coefficients: Vec<f32>) -> Self {
        Self {
            coefficients: coefficients.into(),
        }
    }

    pub fn coefficients(&self) -> &[f32] {
        &self.coefficients
    }
}
```

`Arc<[f32]>` communicates a frozen sequence directly. `Arc<Vec<f32>>` would
retain an inner capacity-and-growth abstraction that this contract does not use.

## Tests

Preserve all functional and numerical tests. Add a focused test that clones
plans and exercises their intended cross-thread use. If performance motivates
the change, benchmark the real construction, clone, access, and drop pattern
before and after; do not infer total memory or throughput from handle size.

## Lesson

Choose the least powerful owner that expresses the required capability:
`Vec<T>` for building and mutation, `Box<[T]>` for a unique frozen sequence,
`Rc<[T]>` for single-thread shared ownership, and `Arc<[T]>` for cross-thread
shared ownership. Cheap shared clones are a benefit only when sharing the same
immutable content is the intended semantics.

## Applies when

- A material sequence is long-lived and shape-immutable after construction.
- Multiple logical owners intentionally share identical contents.
- Clone frequency, allocation, or ownership topology makes the choice material.
- Thread boundaries are known.

## Does not apply when

- The sequence grows, mutates, retains useful spare capacity, or is reused as a
  buffer.
- Each clone must be an independent snapshot.
- The value is small or short-lived enough that a specialized owner adds noise.
- Conversion, reference-count updates, lifetime extension, or cycles outweigh
  the measured benefit.
