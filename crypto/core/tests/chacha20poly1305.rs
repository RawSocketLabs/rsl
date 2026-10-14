//! Public `AEAD_CHACHA20_POLY1305` validation harness.
//!
//! Fixture provenance is recorded in `tests/vectors/chacha20-poly1305/README.md`.

#[path = "chacha20poly1305/boundaries.rs"]
mod boundaries;
#[path = "chacha20poly1305/differential.rs"]
mod differential;
#[path = "chacha20poly1305/known_answers.rs"]
mod known_answers;
#[path = "chacha20poly1305/rfc_fixtures.rs"]
mod rfc_fixtures;
#[path = "chacha20poly1305/support.rs"]
mod support;
#[path = "chacha20poly1305/wycheproof_fixtures.rs"]
mod wycheproof_fixtures;
