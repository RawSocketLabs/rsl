//! A rule that covers a range of IPv4 addresses.

// --- Standard library ---
use std::net::Ipv4Addr;
use std::sync::Arc;

// --- Internal modules ---
use super::{Ports, Protocol, Target, Transport};

/// A rule for a range of IPv4 addresses (spec §6.2, §6.5). A range whose first address is
/// above its last covers nothing.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct IpRule {
    /// The first address covered.
    pub min: Ipv4Addr,
    /// The last address covered.
    pub max: Ipv4Addr,
    /// The ports covered.
    pub ports: Ports,
    /// The protocol covered.
    pub protocol: Protocol,
    /// The application the rule belongs to, shared by all of its rules.
    pub target: Arc<Target>,
}

impl IpRule {
    /// Whether the rule covers `address` reached over `transport` on `port`. ICMP has no
    /// ports, so the port range is not consulted for it.
    #[must_use]
    pub fn covers(&self, address: Ipv4Addr, transport: Transport, port: u16) -> bool {
        (self.min..=self.max).contains(&address)
            && self.protocol.admits(transport)
            && (transport == Transport::Icmp || self.ports.contains(port))
    }
}
