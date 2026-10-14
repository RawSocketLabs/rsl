//! Linux backends over `rustix`. Each handle implements [`RawIo`](crate::RawIo) and adds
//! a typed `send(&impl Protocol)` convenience that encodes (compliant) then transmits.
//!
//! `transport` (L4) is unprivileged; `network` (L3, `IPPROTO_RAW`) and `link` (L2,
//! `AF_PACKET`) need `CAP_NET_RAW`. The crate root re-exports each handle.

#[cfg(feature = "transport")]
pub(crate) mod transport;

#[cfg(feature = "network")]
pub(crate) mod network;

#[cfg(feature = "link")]
pub(crate) mod link;
