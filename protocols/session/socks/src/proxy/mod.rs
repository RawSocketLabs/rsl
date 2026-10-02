//! Complete proxy services: acceptance, session scheduling, DNS, and shutdown.
//!
//! Use a complete proxy when the library should own acceptance and relay. For an
//! individual accepted connection with explicit authorization stages on blocking or
//! Tokio, start with [`crate::server`] instead.
//!
//! Configured blocking servers expose `serve`; Tokio servers expose `into_proxy` and
//! explicit `start`. Backend selection therefore belongs to server configuration, not another
//! proxy builder.
//! Per-connection drivers remain available; established blocking/Tokio
//! relay behavior lives on [`crate::server::Connection`].
//!
//! # Backends
//!
//! Each driver takes its matching configured server and shares the same policy model.
//!
#![cfg_attr(
    feature = "blocking",
    doc = "- [`blocking`]: blocking listeners, per-connection handling, and relay."
)]
#![cfg_attr(
    feature = "tokio",
    doc = "- [`tokio`]: asynchronous listeners, per-connection handling, and relay."
)]

#[cfg(feature = "blocking")]
pub mod blocking;
#[cfg(feature = "tokio")]
pub mod tokio;
