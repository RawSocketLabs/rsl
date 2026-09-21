//! Shared transport mechanics; no protocol negotiation.

mod address;
#[cfg(feature = "blocking")]
pub(crate) mod blocking;
mod deadline;
#[cfg(feature = "mio")]
pub mod mio;
pub(crate) mod stream;
#[cfg(feature = "tokio")]
pub(crate) mod tokio;

pub(crate) use address::domain_name;
#[cfg(feature = "tokio")]
pub(crate) use deadline::timed_out;
