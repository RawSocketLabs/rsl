# Use Type-State Only for Consequential Staging

## Before

A public builder introduces generic marker types for every optional field,
creating many types and diagnostics even though only `address` is required and
`build()` already validates it.

## Review

The state graph does not prevent a dangerous operation; it encodes ordinary
configuration and makes public evolution harder. A conventional builder with a
required constructor argument or structured `build()` error is clearer.

## After

Use `ClientBuilder::new(address)` for the required value, named methods for
optional policy, private fields, documented defaults, and `build() ->
Result<Client, BuildError>` for contextual validation.

## Tests

Compile the common consumer path, test defaults and every validation group,
check public documentation, and run compatibility analysis. If a later staged
resource must never be used before authentication, evaluate narrow type-state
for that transition.

## Lesson

Builders improve named configuration and evolution. Type-state is worthwhile
when a small state graph prevents consequential misuse, not as a default builder
decoration.

## Applies when

- Construction has optional policy and useful defaults.
- Runtime validation can report contextual failure clearly.

## Does not apply when

- Compile-time staging prevents a safety, security, or lifecycle violation.
- The state graph is small, stable, and materially improves callers.
