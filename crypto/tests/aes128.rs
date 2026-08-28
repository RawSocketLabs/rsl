//! Public AES-128 validation harness.
//!
//! Fixture provenance and conversion policy are recorded in
//! `tests/vectors/aes-128/README.md`. These tests exercise only the public API.

#[path = "aes128/differential.rs"]
mod differential;
#[path = "aes128/known_answers.rs"]
mod known_answers;
#[path = "aes128/round_trip.rs"]
mod round_trip;
