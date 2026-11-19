//! The network under the protocol: TCP sockets, TLS and HTTP, for a client that talks to a
//! real gateway. Behind the `net` feature, so that the protocol itself builds without them.
//!
//! [`HttpsPortal`] is the [`Portal`](crate::api::Portal) a login runs over, and [`dial`]
//! opens the connection to a node that a [`tcp::Stream`](crate::tcp::Stream) runs over.
//!
//! Both require an explicit TLS verification policy. With unverified TLS (§2.1) an active
//! attacker can receive the password or session identifier. The protocol's own checks
//! cannot authenticate the gateway.

mod node;
mod portal;
mod socket;

pub use node::dial;
pub use portal::HttpsPortal;
