# Keep Pointer Obligations at the Unsafe Boundary

## Before

```rust
pub fn samples<'a>(ptr: *const i16, len: usize) -> &'a [i16] {
    unsafe { std::slice::from_raw_parts(ptr, len) }
}
```

Safe callers can choose an invalid pointer, length, lifetime, or allocation.

## Review

Null checks cannot prove provenance, alignment, initialization, allocation
extent, lifetime, or concurrent mutation. Those obligations must stay with an
unsafe caller or a foreign callback whose ABI contract establishes them.

## After

Expose a safe API from an existing `&[i16]`. At the FFI entry point, document
the foreign caller's complete pointer/length/lifetime contract, validate
checkable conditions, construct the slice in one small unsafe block, and keep it
inside the established callback lifetime.

## Tests

Test zero length according to the ABI contract, valid buffers, lengths at the
foreign maximum, error mapping, callback lifetime, panic containment, and target
ABI linkage. Run Miri where the allocation is Rust-owned and sanitizers for the
integrated foreign path when available.

## Lesson

A safe function cannot accept raw pointer facts it cannot validate. The safe
wrapper must encode the invariant; the unsafe boundary documents what remains.

## Applies when

- Rust receives foreign pointers, lengths, callbacks, or handles.
- A safe wrapper is being designed around raw memory.

## Does not apply when

- The API accepts an ordinary Rust slice whose lifetime and extent are already
  compiler-checked.
- A deliberately unsafe function exposes precise caller obligations.
