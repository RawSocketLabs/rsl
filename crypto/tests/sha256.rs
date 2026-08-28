//! Public-behavior validation for the readable SHA-256 implementation.
//!
//! The test modules deliberately separate published vectors, padding boundaries, streaming
//! behavior, and differential evidence. Private intermediate-state tests remain beside the
//! implementation that owns each SHA-256 layer.

#[path = "sha256/boundaries.rs"]
mod boundaries;
#[path = "sha256/differential.rs"]
mod differential;
#[path = "sha256/known_answers.rs"]
mod known_answers;
#[path = "sha256/streaming.rs"]
mod streaming;
#[path = "sha256/support.rs"]
mod support;
