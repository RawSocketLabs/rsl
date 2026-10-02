//! Shared transport mechanics; no protocol negotiation.
//!
//! # Buffered handoff
//!
#![cfg_attr(
    any(feature = "blocking", feature = "tokio", feature = "mio"),
    doc = "[`crate::Stream`] retains prefetched bytes when moving from protocol negotiation to application I/O. Its standard and Tokio trait implementations share that buffer."
)]
#![cfg_attr(
    not(any(feature = "blocking", feature = "tokio", feature = "mio")),
    doc = "Transport support is disabled in this build. Wire codecs remain available through [`crate::v5::wire`]."
)]
#![cfg_attr(
    feature = "mio",
    doc = "\n# Readiness-driven transport\n\n[`mio`] provides numeric TCP connection mechanics for caller-owned event loops, independently of SOCKS negotiation."
)]

#[cfg(feature = "blocking")]
pub(crate) mod blocking;
pub(crate) mod deadline;
#[cfg(feature = "mio")]
pub mod mio;
#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
pub(crate) mod stream;
#[cfg(feature = "tokio")]
pub(crate) mod tokio;

// --- Internal modules ---
#[cfg(feature = "tokio")]
pub(crate) use deadline::timed_out;
