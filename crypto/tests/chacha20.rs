//! Public `ChaCha20` validation harness.
//!
//! Fixture provenance is recorded in `tests/vectors/chacha20-poly1305/README.md`.

#[path = "chacha20/known_answers.rs"]
mod known_answers;
#[path = "chacha20/rfc_fixtures.rs"]
mod rfc_fixtures;
#[path = "chacha20/support.rs"]
mod support;
