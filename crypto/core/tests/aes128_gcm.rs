//! Public AES-128-GCM validation harness.
//!
//! Fixture provenance and conversion policy are recorded in
//! `tests/vectors/gcm/README.md`. These tests exercise only the public API.

#[path = "aes128_gcm/differential.rs"]
mod differential;
#[path = "aes128_gcm/known_answers.rs"]
mod known_answers;
#[path = "aes128_gcm/round_trip_and_failure.rs"]
mod round_trip_and_failure;
