//! Public ECDH P-256 validation harness.
//!
//! Fixture provenance and conversion policy are recorded in
//! `tests/vectors/ecdh-p256/README.md`. These tests exercise only the public API.

#[path = "ecdh_p256/boundaries.rs"]
mod boundaries;
#[path = "ecdh_p256/cavp_cdh_fixtures.rs"]
mod cavp_cdh_fixtures;
#[path = "ecdh_p256/cavp_pkv_fixtures.rs"]
mod cavp_pkv_fixtures;
#[path = "ecdh_p256/differential.rs"]
mod differential;
#[path = "ecdh_p256/known_answers.rs"]
mod known_answers;
#[path = "ecdh_p256/support.rs"]
mod support;
