//! The request that follows the opening message.

// --- Standard library ---
use std::net::Ipv4Addr;

// --- Workspace dependencies ---
use bnb::bin;
use socks::v5::Endpoint;

// --- Internal modules ---
use crate::Negotiation;

/// The request a client sends right after its [`Negotiation`](crate::Negotiation), in the
/// layout of a SOCKS5 request (spec §7.1 and §9.1).
///
/// Dual-use: every byte is stored. The third byte, reserved in SOCKS5, is a flag here.
#[bin(big)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TunnelRequest {
    /// The protocol version, 5.
    pub version: u8,
    /// The command.
    pub command: u8,
    /// On a TCP tunnel, 1 to ask for the framed data phase and 0 for the raw one. Zero on a
    /// virtual-address query.
    pub flag: u8,
    /// The destination, or the unspecified address on a virtual-address query.
    #[brw(variable)]
    pub endpoint: Endpoint,
}

impl TunnelRequest {
    /// The command that opens a TCP stream to the destination.
    pub const COMMAND_CONNECT: u8 = 0x01;
    /// The command that asks the node for the client's virtual address.
    pub const COMMAND_QUERY_VIP: u8 = 0x04;

    /// A request to connect to `destination`, with the framed data phase if `framed`.
    #[must_use]
    pub fn connect(destination: Endpoint, framed: bool) -> Self {
        Self {
            version: Negotiation::VERSION,
            command: Self::COMMAND_CONNECT,
            flag: u8::from(framed),
            endpoint: destination,
        }
    }

    /// The request for the client's virtual address: no destination, so the unspecified
    /// IPv4 address and port zero.
    #[must_use]
    pub fn query_vip() -> Self {
        Self {
            version: Negotiation::VERSION,
            command: Self::COMMAND_QUERY_VIP,
            flag: 0,
            endpoint: Endpoint::addr(Ipv4Addr::UNSPECIFIED, 0),
        }
    }
}
