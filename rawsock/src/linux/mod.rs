//! Linux backends over `rustix`. Each handle implements [`RawIo`](crate::RawIo) and adds
//! a typed `send(&impl Protocol)` convenience that encodes (compliant) then transmits.
//!
//! This first cut ships only the unprivileged `transport` (L4) backend; the privileged
//! `network` (L3, `IPPROTO_RAW`) and `link` (L2, `AF_PACKET`) backends land with the
//! header-forging protocols — see `ROADMAP.md`.

#[cfg(feature = "transport")]
pub mod transport;

#[cfg(feature = "network")]
pub mod network;
