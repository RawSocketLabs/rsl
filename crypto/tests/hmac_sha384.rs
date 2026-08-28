//! Public-behavior validation for the readable HMAC-SHA-384 implementation.
//!
//! Published vectors, streaming behavior, verification failures, and differential evidence are
//! kept in separate modules. Private key and pad transformations remain beside their source.

#[path = "hmac_sha384/differential.rs"]
mod differential;
#[path = "hmac_sha384/known_answers.rs"]
mod known_answers;
#[path = "hmac_sha384/streaming.rs"]
mod streaming;
#[path = "hmac_sha384/support.rs"]
mod support;
#[path = "hmac_sha384/verification.rs"]
mod verification;
