//! Public Poly1305 validation harness.
//!
//! Fixture provenance is recorded in `tests/vectors/chacha20-poly1305/README.md`.

#[path = "poly1305/known_answers.rs"]
mod known_answers;
#[path = "poly1305/rfc_fixtures.rs"]
mod rfc_fixtures;
#[path = "poly1305/support.rs"]
mod support;
