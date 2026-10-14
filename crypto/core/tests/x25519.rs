//! Public X25519 validation harness.
//!
//! Fixture provenance and conversion policy are recorded in
//! `tests/vectors/x25519/README.md`. These tests exercise only the public API.

#[path = "x25519/boundaries.rs"]
mod boundaries;
#[path = "x25519/differential.rs"]
mod differential;
#[path = "x25519/known_answers.rs"]
mod known_answers;
#[path = "x25519/support.rs"]
mod support;
