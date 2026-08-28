//! Public ECDSA P-384/SHA-384 validation harness.
//!
//! Fixture provenance and conversion policy are recorded in
//! `tests/vectors/ecdsa-p384/README.md`. These tests exercise only the public API.

#[path = "ecdsa_p384/boundaries.rs"]
mod boundaries;
#[path = "ecdsa_p384/cavp_siggen_fixtures.rs"]
mod cavp_siggen_fixtures;
#[path = "ecdsa_p384/cavp_sigver_fixtures.rs"]
mod cavp_sigver_fixtures;
#[path = "ecdsa_p384/differential.rs"]
mod differential;
#[path = "ecdsa_p384/known_answers.rs"]
mod known_answers;
#[path = "ecdsa_p384/support.rs"]
mod support;
