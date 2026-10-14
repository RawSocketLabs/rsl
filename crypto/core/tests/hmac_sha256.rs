//! Public-behavior validation for the readable HMAC-SHA-256 implementation.
//!
//! Published vectors, streaming behavior, verification failures, and differential evidence are
//! kept in separate modules. Private key and pad transformations remain beside their source.

#[path = "hmac_sha256/differential.rs"]
mod differential;
#[path = "hmac_sha256/known_answers.rs"]
mod known_answers;
#[path = "hmac_sha256/streaming.rs"]
mod streaming;
#[path = "hmac_sha256/support.rs"]
mod support;
#[path = "hmac_sha256/verification.rs"]
mod verification;
