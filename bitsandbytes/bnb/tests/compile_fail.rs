//! Compile-fail UI tests for `#[bin]` / `#[bitfield]`: every misuse must be rejected
//! with a clear, well-spanned diagnostic (not a confusing error from generated code).
//!
//! Snapshots live in `tests/ui/*.stderr`; regenerate with `TRYBUILD=overwrite`.

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
