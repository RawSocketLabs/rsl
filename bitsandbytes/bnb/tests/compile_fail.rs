//! Compile-fail UI tests for `#[bin]` / `#[bitfield]`: every misuse must be rejected
//! with a clear, well-spanned diagnostic (not a confusing error from generated code).
//!
//! Snapshots live in `tests/ui/*.stderr`; regenerate with `TRYBUILD=overwrite`.

#[test]
fn ui() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}

/// The `restore_position`→`SeekSource` diagnostic lists the in-scope `SeekSource`
/// impls, which the `bytes` feature extends (it adds `BytesReader`). Pin this case
/// to the default feature set so the snapshot is stable across configs.
#[cfg(not(feature = "bytes"))]
#[test]
fn ui_seek() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui_seek/*.rs");
}

/// `DatagramSocket` (the `net` feature) is sealed: a downstream `impl` for an outside type is a
/// compile error. Pinned to the `mock` feature so the diagnostic's in-scope impl set is stable
/// (mock implies net and adds `MockDatagramSocket`).
#[cfg(feature = "mock")]
#[test]
fn ui_seal() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui_seal/*.rs");
}
