//! The network under the protocol: TCP sockets, TLS and HTTP, for a client that talks to a
//! real gateway. Behind the `net` feature, so that the protocol itself builds without them.
//!
//! [`HttpsPortal`] is the [`Portal`](crate::api::Portal) a login runs over, and [`dial`]
//! opens the connection to a node that a [`tcp::Stream`](crate::tcp::Stream) runs over.
//!
//! Neither verifies the peer's certificate (spec §2.1): gateways and nodes present
//! self-signed ones. What stands in for that on the control plane is the protocol's own
//! anti-MITM checks, whose outcome a login reports. A node connection has no such check.

mod node;
mod portal;
mod socket;

pub use node::dial;
pub use portal::HttpsPortal;
