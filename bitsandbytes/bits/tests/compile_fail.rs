//! Compile-fail UI tests for `#[wire]`: every misuse must be rejected with a
//! clear, well-spanned diagnostic (not a confusing error from generated code).
//!
//! Snapshots live in `tests/ui/*.stderr`; regenerate with `TRYBUILD=overwrite`.
#![cfg(feature = "binrw")]

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
