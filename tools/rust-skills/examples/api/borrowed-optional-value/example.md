# Borrow the Optional Referent

## Before

```rust
pub fn render_label(label: &Option<String>) -> String {
    match label.as_ref() {
        Some(label) => format!("[{label}]"),
        None => "[untitled]".to_owned(),
    }
}
```

The callee only observes an optional string slice, but the signature requires
the caller to store it as exactly `Option<String>`.

## Review

The conceptual input is an optional borrowed string. Accepting
`Option<&str>` allows all of these callers without transferring ownership:

```rust
render_label(Some(command_line_label.as_str()));
render_label(settings.label.as_deref());
render_label(None);
```

The optional reference is a value. Matching or using a consuming `Option`
combinator consumes that small value, not the string it refers to. The API also
stops exposing whether a settings object stores `String`, `Box<str>`, or another
owner.

This is an API-design observation, not a size micro-optimization. Representation
guarantees are secondary to decoupling the borrow from its storage container.

## After

```rust
pub fn render_label(label: Option<&str>) -> String {
    match label {
        Some(label) => format!("[{label}]"),
        None => "[untitled]".to_owned(),
    }
}
```

## Tests

Test direct values, stored optional owners, and absence. If this changes a
public API, run the repository's compatibility checks and document the
migration. Compile-time call-shape tests are useful when accepting all three
forms is part of the contract.

## Lesson

For a read-only optional borrowed input or output, expose the optional referent:
`Option<&T>`, and preferably the deepest useful abstraction such as
`Option<&str>`. Adapt owned storage with `as_ref` or `as_deref`. If ownership is
needed, request it or create it explicitly at the ownership boundary.

## Applies when

- A new parameter or return value only observes an optional referent.
- The caller may have a direct value, an owned optional value, or no value.
- The API should hide the owner's exact storage representation.

## Does not apply when

- `&mut Option<T>` is needed to take, insert, replace, or otherwise change
  presence.
- Pinning, a required trait, FFI, or another external contract fixes the
  signature.
- Compatibility makes an existing public signature more important than a
  cleanup.
- A transient internal `&Option<T>` does not escape or constrain an API.
