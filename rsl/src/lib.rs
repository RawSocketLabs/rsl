//! # `rsl` — the RawSocket Labs stack
//!
//! A single, feature-gated facade over the crates RSL application layers build on.
//! It re-exports two kinds of dependency under one namespace:
//!
//! - **Owned RSL crates** live under semantic paths — [`codec`], [`proto`], [`rawsock`],
//!   [`rf`], and (FFI, opt-in) [`usdr`]. These are first-class members of the stack. The
//!   private owned crates (libsdr, rust-dsdcc) are deliberately absent from this public
//!   facade; bless them in a private overlay crate that depends on `rsl`.
//! - **Blessed external crates** live under [`ext`] (e.g. `rsl::ext::serde`). The `ext`
//!   prefix marks them as *replacement candidates*: when RSL writes an owned crate to
//!   supersede one, it graduates out of `ext` into a semantic path and the pin is dropped
//!   from `Cargo.toml`. Consumers importing `rsl::ext::foo` see the move; the version bump
//!   itself happens in exactly one place.
//!
//! ## Why consume the stack through `rsl`
//!
//! - **One place to change versions.** Every blessed pin lives in this crate's
//!   `[dependencies]`. Application crates depend on `rsl`, not on the stack directly.
//! - **Pick only what you use.** Nothing is compiled until you enable a feature; `rsl`
//!   with default features is empty. See the module docs and `README.md` for the feature
//!   map and the convenience bundles (`full`, `net`, `radio`, `std-ext`).
//! - **A curated, legible stack.** The set of blessed externals is exactly the `ext`
//!   namespace — a standing answer to "what do we depend on, and what might we replace?"
//!
//! ```toml
//! # In an application crate:
//! rsl = { git = "https://github.com/RawSocketLabs/rsl", features = ["net", "std-ext"] }
//! ```

// Owned crates are the stack; externals are provisional. Re-exports don't carry docs of
// their own, so we don't `deny(missing_docs)` here.

// ===========================================================================
// Internal: owned RSL crates, under semantic namespaces.
// ===========================================================================

/// Bit-aware binary codec (`bitsandbytes`, imported as `bnb`) — the foundation every
/// on-the-wire type is built on.
#[cfg(feature = "codec")]
#[doc(inline)]
pub use bnb as codec;

/// Layered raw-packet I/O: transmit arbitrary bytes at L2/L3/L4.
#[cfg(feature = "rawsock")]
#[doc(inline)]
pub use rawsock;

/// RF frequency, sample-rate, and scan-target parsing utilities (`rfus`).
#[cfg(feature = "rf")]
#[doc(inline)]
pub use rfus as rf;

/// USDR software-defined-radio bindings (FFI; requires a C++ toolchain to build).
#[cfg(feature = "usdr")]
#[doc(inline)]
pub use usdr;

/// Network-protocol implementations, one submodule per protocol. Each is a from-scratch,
/// dual-use (compliant-by-default, deliberately violatable) implementation built on
/// [`crate::codec`].
#[cfg(any(
    feature = "proto-ethertype",
    feature = "proto-tcp",
    feature = "proto-udp",
    feature = "proto-dns",
))]
pub mod proto {
    /// EtherType link-layer definitions.
    #[cfg(feature = "proto-ethertype")]
    #[doc(inline)]
    pub use ::ethertype;

    /// TCP.
    #[cfg(feature = "proto-tcp")]
    #[doc(inline)]
    pub use ::tcp;

    /// UDP.
    #[cfg(feature = "proto-udp")]
    #[doc(inline)]
    pub use ::udp;

    /// DNS.
    #[cfg(feature = "proto-dns")]
    #[doc(inline)]
    pub use ::dns;
}

// ===========================================================================
// External: blessed third-party crates. Replacement candidates live here.
// ===========================================================================

/// Blessed external crates, re-exported under their canonical names. Everything in this
/// module is a candidate for eventual replacement by an owned RSL crate; when that
/// happens the replacement graduates to a semantic path outside `ext`.
#[cfg(any(
    feature = "error",
    feature = "log",
    feature = "serde",
    feature = "bytes",
    feature = "cli",
    feature = "rand",
    feature = "hash",
    feature = "netutil",
    feature = "parse",
    feature = "num",
    feature = "audio",
    feature = "fft",
    feature = "async",
    feature = "nats",
    feature = "parallel",
    feature = "web-server",
    feature = "openapi",
    feature = "http",
    feature = "tui",
    feature = "time",
    feature = "protobuf",
))]
pub mod ext {
    /// Derive-based error types.
    #[cfg(feature = "error")]
    #[doc(inline)]
    pub use ::thiserror;
    /// Boxed, contextual error handling for application code.
    #[cfg(feature = "error")]
    #[doc(inline)]
    pub use ::anyhow;

    /// Structured, leveled application logging/tracing.
    #[cfg(feature = "log")]
    #[doc(inline)]
    pub use ::tracing;
    /// Tracing subscriber (fmt + env-filter + json) for binaries and tests.
    #[cfg(feature = "log")]
    #[doc(inline)]
    pub use ::tracing_subscriber;
    /// Non-blocking file/rolling appender for `tracing`.
    #[cfg(feature = "log")]
    #[doc(inline)]
    pub use ::tracing_appender;

    /// Serialization framework (config, logs — wire formats use [`crate::codec`]).
    #[cfg(feature = "serde")]
    #[doc(inline)]
    pub use ::serde;
    /// JSON support for `serde`.
    #[cfg(feature = "serde")]
    #[doc(inline)]
    pub use ::serde_json;

    /// Efficient byte buffers.
    #[cfg(feature = "bytes")]
    #[doc(inline)]
    pub use ::bytes;

    /// Command-line argument parsing (derive-based) for binary crates.
    #[cfg(feature = "cli")]
    #[doc(inline)]
    pub use ::clap;

    /// Random number generation.
    #[cfg(feature = "rand")]
    #[doc(inline)]
    pub use ::rand;

    /// SHA-2 hashing.
    #[cfg(feature = "hash")]
    #[doc(inline)]
    pub use ::sha2;
    /// CRC-32 checksums.
    #[cfg(feature = "hash")]
    #[doc(inline)]
    pub use ::crc32fast;

    /// MAC-address types.
    #[cfg(feature = "netutil")]
    #[doc(inline)]
    pub use ::macaddr;
    /// Low-level socket configuration.
    #[cfg(feature = "netutil")]
    #[doc(inline)]
    pub use ::socket2;

    /// Parser-combinator library.
    #[cfg(feature = "parse")]
    #[doc(inline)]
    pub use ::winnow;

    /// Complex numbers (DSP / SDR sample math).
    #[cfg(feature = "num")]
    #[doc(inline)]
    pub use ::num_complex;

    /// WAV audio I/O.
    #[cfg(feature = "audio")]
    #[doc(inline)]
    pub use ::hound;

    /// High-performance FFT (SDR spectrum / scanner math).
    #[cfg(feature = "fft")]
    #[doc(inline)]
    pub use ::rustfft;

    /// Async runtime.
    #[cfg(feature = "async")]
    #[doc(inline)]
    pub use ::tokio;
    /// Async combinators (streams, sinks) that pair with `tokio`.
    #[cfg(feature = "async")]
    #[doc(inline)]
    pub use ::futures_util;

    /// NATS messaging client.
    #[cfg(feature = "nats")]
    #[doc(inline)]
    pub use ::async_nats;

    /// Data-parallel iterators / thread pool.
    #[cfg(feature = "parallel")]
    #[doc(inline)]
    pub use ::rayon;

    /// HTTP server framework.
    #[cfg(feature = "web-server")]
    #[doc(inline)]
    pub use ::axum;
    /// Service/middleware abstractions under `axum`.
    #[cfg(feature = "web-server")]
    #[doc(inline)]
    pub use ::tower;
    /// HTTP middleware (tracing, etc.) for `tower`/`axum`.
    #[cfg(feature = "web-server")]
    #[doc(inline)]
    pub use ::tower_http;

    /// OpenAPI derive.
    #[cfg(feature = "openapi")]
    #[doc(inline)]
    pub use ::utoipa;
    /// Swagger UI serving for `utoipa`.
    #[cfg(feature = "openapi")]
    #[doc(inline)]
    pub use ::utoipa_swagger_ui;

    /// HTTP client.
    #[cfg(feature = "http")]
    #[doc(inline)]
    pub use ::reqwest;

    /// Terminal UI framework.
    #[cfg(feature = "tui")]
    #[doc(inline)]
    pub use ::ratatui;
    /// Colorful panic/error reports for TUI/CLI binaries.
    #[cfg(feature = "tui")]
    #[doc(inline)]
    pub use ::color_eyre;

    /// Date and time.
    #[cfg(feature = "time")]
    #[doc(inline)]
    pub use ::chrono;

    /// Protocol Buffers runtime.
    #[cfg(feature = "protobuf")]
    #[doc(inline)]
    pub use ::prost;
}

/// Common imports. Glob this in to pull the everyday items of whatever features are
/// enabled — `use rsl::prelude::*;`.
pub mod prelude {
    #[cfg(feature = "error")]
    #[doc(no_inline)]
    pub use crate::ext::anyhow::{Context as _, Result as AnyhowResult};
    #[cfg(feature = "error")]
    #[doc(no_inline)]
    pub use crate::ext::thiserror::Error as ThisError;

    #[cfg(feature = "log")]
    #[doc(no_inline)]
    pub use crate::ext::tracing::{debug, error, info, trace, warn};

    #[cfg(feature = "serde")]
    #[doc(no_inline)]
    pub use crate::ext::serde::{Deserialize, Serialize};

    #[cfg(feature = "codec")]
    #[doc(no_inline)]
    pub use crate::codec;
}
