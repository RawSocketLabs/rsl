//! What a node answers on either kind of tunnel connection, one type per step.

// --- Standard library ---
use std::net::{Ipv4Addr, Ipv6Addr};

// --- Workspace dependencies ---
use bnb::bin;

/// The node's answer to the greeting: the version and the method it accepts (spec §7.2 and
/// §9.1). A compliant node echoes the method the client offered.
#[bin(big)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MethodReply {
    /// The protocol version, 5.
    pub version: u8,
    /// The method accepted.
    pub method: u8,
}

/// The node's answer to the sub-negotiation: a marker, a status, and a JSON document that
/// may be empty (spec §7.2 and §9.1).
///
/// The whole document must be read before acting on the status, or the stream loses its
/// framing; decoding this type does that.
#[bin(big)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AuthReply {
    /// The sub-negotiation marker, `'S'`.
    pub marker: u8,
    /// The status: zero for success.
    pub status: u8,
    /// The JSON document, uninterpreted.
    #[brw(count_prefix = u16)]
    pub json: Vec<u8>,
}

/// The first three bytes of the node's answer to a virtual-address query (spec §7.2).
///
/// The specification does not say what follows a status other than zero, so these are decoded
/// on their own and the [`VipAddresses`] are read only after a success.
#[bin(big)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VipHead {
    /// The protocol version, 5.
    pub version: u8,
    /// The status: zero for success.
    pub status: u8,
    /// A byte whose value varies between nodes and carries no known meaning.
    pub unused: u8,
}

/// The addresses a node assigns, after a successful [`VipHead`]: an address-type byte, then
/// the form it selects (spec §7.2). Each form ends in two bytes whose meaning is not known;
/// they are kept so the message round-trips. An address type other than these three has no
/// defined length and does not decode.
#[bin(big)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VipAddresses {
    /// An IPv4 virtual address.
    #[bin(magic = 0x01u8)]
    V4 {
        /// The address.
        address: Ipv4Addr,
        /// Two trailing bytes of unknown meaning.
        trailer: [u8; 2],
    },
    /// An IPv6 virtual address.
    #[bin(magic = 0x04u8)]
    V6 {
        /// The address.
        address: Ipv6Addr,
        /// Two trailing bytes of unknown meaning.
        trailer: [u8; 2],
    },
    /// Both.
    #[bin(magic = 0x05u8)]
    Both {
        /// The IPv4 address.
        v4: Ipv4Addr,
        /// The IPv6 address.
        v6: Ipv6Addr,
        /// Two trailing bytes of unknown meaning.
        trailer: [u8; 2],
    },
}
