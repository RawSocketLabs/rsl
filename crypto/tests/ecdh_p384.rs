//! Public ECDH P-384 validation harness.
//!
//! Fixture provenance and conversion policy are recorded in
//! `tests/vectors/ecdh-p384/README.md`. These tests exercise only the public API.

#[path = "ecdh_p384/boundaries.rs"]
mod boundaries;
#[path = "ecdh_p384/cavp_cdh_fixtures.rs"]
mod cavp_cdh_fixtures;
#[path = "ecdh_p384/cavp_pkv_fixtures.rs"]
mod cavp_pkv_fixtures;
#[path = "ecdh_p384/differential.rs"]
mod differential;
#[path = "ecdh_p384/known_answers.rs"]
mod known_answers;
#[path = "ecdh_p384/support.rs"]
mod support;
