//! Public ECDSA P-256/SHA-256 validation harness.
//!
//! Fixture provenance and conversion policy are recorded in
//! `tests/vectors/ecdsa-p256/README.md`. These tests exercise only the public API.

#[path = "ecdsa_p256/boundaries.rs"]
mod boundaries;
#[path = "ecdsa_p256/cavp_siggen_fixtures.rs"]
mod cavp_siggen_fixtures;
#[path = "ecdsa_p256/cavp_sigver_fixtures.rs"]
mod cavp_sigver_fixtures;
#[path = "ecdsa_p256/differential.rs"]
mod differential;
#[path = "ecdsa_p256/known_answers.rs"]
mod known_answers;
#[path = "ecdsa_p256/support.rs"]
mod support;
