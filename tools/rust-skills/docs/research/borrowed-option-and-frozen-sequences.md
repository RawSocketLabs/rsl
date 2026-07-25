# Borrowed Optional Values and Frozen Sequence Ownership

Status: reviewed synthesis implemented in the modular runtime packages

## Provenance and authority

Logan Smith's videos supplied the challenge and useful API examples. Their
wording is not copied, and the videos are advisory rather than authoritative.
The Rust standard-library documentation establishes the language and library
facts. Clippy's `ref_option` lint independently recognizes the borrowed-option
signature issue and links to Smith's explanation. RSL decisions R167-R171 state
the policy adopted here.

## Optional borrowed API boundaries

### Facts

- `Option<T>` provides `as_ref`, `as_mut`, `as_deref`, and `as_deref_mut` to
  adapt an owned optional value without moving it.
- `Option<&T>` is itself a small value whose contained shared reference is
  `Copy`. Consuming `Option` combinators therefore consume the optional
  reference, not the referent.
- For sized `T`, Rust guarantees the null-pointer optimization for `Option<&T>`.
  This representation fact is useful but is not the main API-design reason.

### Complete argument

An API that only observes an optional borrowed value should describe that value,
not require the caller's exact storage representation. `Option<&T>` accepts a
direct referent with `Some(&value)`, a stored `Option<T>` with
`stored.as_ref()`, or `None`. In contrast, `&Option<T>` requires the caller to
possess that exact container and couples returned references to its
representation.

Borrowing the deepest useful abstraction preserves more implementation freedom.
An accessor returning `Option<&str>` can continue to do so if storage changes
from `Option<String>` to `Option<Box<str>>`. The same principle motivates
`&[T]` over `&Vec<T>`, `&T` over `&Box<T>`, and `&Path` over `&PathBuf` when the
callee needs only the referent contract.

If the callee needs ownership, the API should request ownership honestly. A
borrowed result can become owned explicitly with `copied`, `cloned`, `to_owned`,
or a mapping operation at the ownership boundary.

### Limits and exceptions

The slogan is not a ban on every value whose Rust type happens to be
`&Option<T>`. A transient internal borrow may be harmless. `&mut Option<T>` is
the right contract when the callee changes whether a value is present, takes the
owned value, or replaces it. Pinning, required traits, FFI, stable public API
compatibility, and rare exact-container contracts can also require a container
reference. A public migration is not a mechanical lint fix; it may be breaking.

## Frozen sequence ownership

### Facts

- Moving a `Vec<T>` moves its allocation handle; it does not copy every element.
- Cloning a `Vec<T>` creates independent element storage, subject to `T: Clone`.
- Cloning `Rc<T>` or `Arc<T>` creates another owner of the same allocation.
  `Arc` uses atomic reference counting and supports cross-thread shared ownership
  when `T` has the required thread-safety properties. `Rc` avoids atomic
  counting and is not for cross-thread ownership.
- `Rc` and `Arc` can participate in cycles, and shared ownership can extend
  lifetimes. They also support limited mutation through unique access or
  copy-on-write operations; calling them immutable is a design shorthand, not a
  complete semantic description.

### Capability matrix

| Required capability | Default candidate |
|---|---|
| Build, grow, mutate, retain capacity, or reuse a buffer | `Vec<T>` |
| Freeze shape under one unique owner | `Box<[T]>` |
| Share frozen data among single-threaded owners | `Rc<[T]>` |
| Share frozen data across threads or a `Send`/`Sync` boundary | `Arc<[T]>` |

This is a decision table, not an automatic rewrite. Long lifetime, material data
size, and repeated logical clones make the choice consequential. Small or
short-lived values often do not.

When frozen shared content is the contract, `Rc<[T]>` or `Arc<[T]>` usually
communicates that contract better than reference-counting a `Vec<T>`.
`Rc<str>` or `Arc<str>` similarly avoids retaining a `String` layer whose
capacity and mutation API are unused. Keep the nested owned container when
those capabilities or an external interface are real requirements.

### Costs and semantic change

Reference counting replaces independent deep copies with shared ownership. That
can reduce copied data and make logical clones cheap, but it adds a reference
count header, count updates on clone and drop, possible atomic overhead for
`Arc`, cycle risk, different lifetime behavior, and conversion or construction
cost. A fat pointer or handle size is not a proof of lower total memory use.
Cache, contention, branch, code-size, and allocation effects are workload and
target dependent.

Use `Rc` when sharing is single-threaded, `Arc` when cross-thread ownership is
required, and `Box` when cloning is not required. Keep `Vec` when independent
ownership, mutation, growth, or buffer reuse is the actual design. Measure the
relevant workload before claiming a performance improvement.

## Implemented ownership

- `rust-api-design` owns R167, the capability model in R168, and R171.
- `rust-performance` owns measurement and cost analysis for R168-R170.
- the RSL organization layer owns the stronger preference in R169;
- `rust-review` routes to those owners and enforces R170 without duplicating
  their full rules.
- `rust-testing` owns behavior, allocation, and benchmark evidence used to
  verify a change.
