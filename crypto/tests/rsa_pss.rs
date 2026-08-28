//! Public RSASSA-PSS SHA-256 validation harness.
//!
//! Fixture provenance and conversion policy are recorded in `tests/vectors/rsa-pss/README.md`.
//! These tests exercise only the public API.

#[path = "rsa_pss/cavp_fixtures.rs"]
mod cavp_fixtures;
#[path = "rsa_pss/known_answers.rs"]
mod known_answers;
#[path = "rsa_pss/support.rs"]
mod support;
#[path = "rsa_pss/wycheproof_fixtures.rs"]
mod wycheproof_fixtures;
