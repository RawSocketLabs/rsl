//! **sangfor-atrust-wire** — the data-plane wire formats of the Sangfor aTrust client
//! protocol, as [`bnb`] codecs: what a client and a node exchange when a tunnel connection
//! opens, and the frames of a TCP tunnel's data phase.
//!
//! The formats are shaped like SOCKS5 with private method numbers and a JSON document in the
//! authentication step. They are specified in §7 and §9 of `../SPEC-atrust.md`. This crate is
//! byte layout only: it does no I/O, builds no JSON and checks no status code. Every field is
//! stored verbatim; the constructors fill in what a compliant client sends.
//!
//! What both kinds of tunnel connection share is at the crate root; what only a TCP tunnel
//! uses is in [`tcp`]. The L3 tunnel's frames (specification §8) are not here yet.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod negotiation;
mod reply;
mod request;
pub mod tcp;

pub use negotiation::Negotiation;
pub use reply::{AuthReply, MethodReply, VipAddresses, VipHead};
pub use request::TunnelRequest;
pub use socks::v5::Endpoint;
