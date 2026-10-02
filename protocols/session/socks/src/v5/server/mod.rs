//! SOCKS5 server implementations, grouped by I/O backend.
//!
//! These are embedded exchanges, not complete proxies. Their caller owns authorization,
//! dialing the target, and the decision to send success or failure.
//!
//! # Backends
//!
#![cfg_attr(
    feature = "blocking",
    doc = "- [`blocking`]: CONNECT negotiation and caller-controlled replies."
)]
#![cfg_attr(
    feature = "tokio",
    doc = "- [`tokio`]: async CONNECT negotiation and caller-controlled replies."
)]

#[cfg(feature = "blocking")]
pub mod blocking;
#[cfg(feature = "tokio")]
pub mod tokio;
mod validation;
