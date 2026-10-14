//! Public AES-256-GCM validation harness.
//!
//! Fixture provenance and conversion policy are recorded in `tests/vectors/gcm/README.md`.

#[path = "aes256_gcm/differential.rs"]
mod differential;
#[path = "aes256_gcm/known_answers.rs"]
mod known_answers;
#[path = "aes256_gcm/nist_fixtures.rs"]
mod nist_fixtures;
#[path = "aes256_gcm/support.rs"]
mod support;
#[path = "aes256_gcm/wycheproof_fixtures.rs"]
mod wycheproof_fixtures;
