//! # `rsl` — the RawSocket Labs owned-library facade
//!
//! A single, feature-gated re-export of the public libraries RSL *owns*, under semantic
//! paths — [`codec`], [`proto`], [`rawsock`], [`rf`], and (FFI, opt-in) [`usdr`]. Depend on
//! `rsl` to consume RSL's own products through one crate with unified, pinned versions.
//!
//! This is the **owned** half of the stack. The **rented** third-party half lives in the
//! separate [`rsl-deps`] crate (`rsl_deps::tokio`, `rsl_deps::serde`, …). The two are
//! orthogonal: a crate needing both blessed externals *and* RSL libraries depends on both.
//! The private owned crates (libsdr, rust-dsdcc) live in the private `rsl-private` overlay.
//!
//! When a rented crate in `rsl-deps` is superseded by an owned RSL crate, the pin moves
//! *out* of `rsl-deps` and the owned crate is added *here* — that cross-crate move is the
//! "graduation" from rented to owned.
//!
//! ```toml
//! # In an application crate (add rsl-deps too if you want blessed externals):
//! rsl = { git = "https://github.com/RawSocketLabs/rsl", features = ["net"] }
//! ```
//!
//! [`rsl-deps`]: https://github.com/RawSocketLabs/rsl-deps

// Re-exports don't carry docs of their own, so we don't `deny(missing_docs)` here.

// ===========================================================================
// Owned RSL crates, under semantic namespaces.
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

/// Common imports from the owned libraries. Glob this in to pull the everyday items of
/// whatever features are enabled — `use rsl::prelude::*;`. Blessed externals have their own
/// prelude in `rsl_deps::prelude`.
pub mod prelude {
    #[cfg(feature = "codec")]
    #[doc(no_inline)]
    pub use crate::codec;
}
