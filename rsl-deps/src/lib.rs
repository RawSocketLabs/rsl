//! # `rsl-deps` — the RSL blessed external-dependency stack
//!
//! One feature-gated crate that pins and re-exports the third-party crates RawSocket Labs
//! libraries and applications build on. Depend on `rsl-deps` instead of pinning `thiserror`,
//! `tokio`, `serde`, … by hand in every crate; versions are unified here, in one place.
//!
//! Each blessed crate is re-exported under its canonical name (`rsl_deps::tokio`,
//! `rsl_deps::serde`, …) behind a feature. Nothing compiles until you enable a feature.
//!
//! ```toml
//! rsl-deps = { git = "https://github.com/RawSocketLabs/rsl-deps", features = ["std-ext", "service"] }
//! ```
//!
//! ```ignore
//! use rsl_deps::prelude::*;   // anyhow::Result, tracing macros, serde derives (feature "std-ext")
//! use rsl_deps::tokio;        // blessed async runtime (feature "async")
//! ```
//!
//! ## Rented, not owned
//!
//! Everything here is third-party — the "rented" half of the RSL stack, and a standing list
//! of replacement candidates. RSL's *owned* libraries live in the separate [`rsl`] facade
//! (`https://github.com/RawSocketLabs/rsl`). When an owned crate supersedes one of these, the
//! pin is removed here and the owned crate is added to `rsl`.
//!
//! [`rsl`]: https://github.com/RawSocketLabs/rsl

// Re-exports don't carry docs of their own, so we don't `deny(missing_docs)` here.

/// Derive-based error types.
#[cfg(feature = "error")]
#[doc(inline)]
pub use anyhow;
/// Boxed, contextual error handling for application code.
#[cfg(feature = "error")]
#[doc(inline)]
pub use thiserror;

/// Structured, leveled application logging/tracing.
#[cfg(feature = "log")]
#[doc(inline)]
pub use tracing;
/// Non-blocking file/rolling appender for `tracing`.
#[cfg(feature = "log")]
#[doc(inline)]
pub use tracing_appender;
/// Tracing subscriber (fmt + env-filter + json) for binaries and tests.
#[cfg(feature = "log")]
#[doc(inline)]
pub use tracing_subscriber;

/// Serialization framework (config, logs — binary wire formats use `rsl::codec`).
#[cfg(feature = "serde")]
#[doc(inline)]
pub use serde;
/// JSON support for `serde`.
#[cfg(feature = "serde")]
#[doc(inline)]
pub use serde_json;

/// Efficient byte buffers.
#[cfg(feature = "bytes")]
#[doc(inline)]
pub use bytes;

/// Command-line argument parsing (derive-based) for binary crates.
#[cfg(feature = "cli")]
#[doc(inline)]
pub use clap;

/// Random number generation.
#[cfg(feature = "rand")]
#[doc(inline)]
pub use rand;

/// CRC-32 checksums.
#[cfg(feature = "hash")]
#[doc(inline)]
pub use crc32fast;
/// SHA-2 hashing.
#[cfg(feature = "hash")]
#[doc(inline)]
pub use sha2;

/// MAC-address types.
#[cfg(feature = "netutil")]
#[doc(inline)]
pub use macaddr;
/// Low-level socket configuration.
#[cfg(feature = "netutil")]
#[doc(inline)]
pub use socket2;

/// Parser-combinator library.
#[cfg(feature = "parse")]
#[doc(inline)]
pub use winnow;

/// Complex numbers (DSP / SDR sample math).
#[cfg(feature = "num")]
#[doc(inline)]
pub use num_complex;

/// WAV audio I/O.
#[cfg(feature = "audio")]
#[doc(inline)]
pub use hound;

/// High-performance FFT (SDR spectrum / scanner math).
#[cfg(feature = "fft")]
#[doc(inline)]
pub use rustfft;

/// Async combinators (streams, sinks) that pair with `tokio`.
#[cfg(feature = "async")]
#[doc(inline)]
pub use futures_util;
/// Async runtime.
#[cfg(feature = "async")]
#[doc(inline)]
pub use tokio;

/// NATS messaging client.
#[cfg(feature = "nats")]
#[doc(inline)]
pub use async_nats;

/// Data-parallel iterators / thread pool.
#[cfg(feature = "parallel")]
#[doc(inline)]
pub use rayon;

/// HTTP server framework.
#[cfg(feature = "web-server")]
#[doc(inline)]
pub use axum;
/// Service/middleware abstractions under `axum`.
#[cfg(feature = "web-server")]
#[doc(inline)]
pub use tower;
/// HTTP middleware (tracing, etc.) for `tower`/`axum`.
#[cfg(feature = "web-server")]
#[doc(inline)]
pub use tower_http;

/// OpenAPI derive.
#[cfg(feature = "openapi")]
#[doc(inline)]
pub use utoipa;
/// Swagger UI serving for `utoipa`.
#[cfg(feature = "openapi")]
#[doc(inline)]
pub use utoipa_swagger_ui;

/// HTTP client.
#[cfg(feature = "http")]
#[doc(inline)]
pub use reqwest;

/// Colorful panic/error reports for TUI/CLI binaries.
#[cfg(feature = "tui")]
#[doc(inline)]
pub use color_eyre;
/// Terminal UI framework.
#[cfg(feature = "tui")]
#[doc(inline)]
pub use ratatui;

/// Date and time.
#[cfg(feature = "time")]
#[doc(inline)]
pub use chrono;

/// Protocol Buffers runtime.
#[cfg(feature = "protobuf")]
#[doc(inline)]
pub use prost;

/// Common imports. Glob this in to pull the everyday items of whatever features are
/// enabled — `use rsl_deps::prelude::*;`.
pub mod prelude {
    #[cfg(feature = "error")]
    #[doc(no_inline)]
    pub use crate::anyhow::{Context as _, Result as AnyhowResult};
    #[cfg(feature = "error")]
    #[doc(no_inline)]
    pub use crate::thiserror::Error as ThisError;

    #[cfg(feature = "log")]
    #[doc(no_inline)]
    pub use crate::tracing::{debug, error, info, trace, warn};

    #[cfg(feature = "serde")]
    #[doc(no_inline)]
    pub use crate::serde::{Deserialize, Serialize};
}
