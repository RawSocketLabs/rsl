//! The protocol of a rule and of a destination.

/// The protocol a rule applies to: an address's `protocol` member (spec §6.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Protocol {
    /// TCP only.
    Tcp,
    /// UDP only.
    Udp,
    /// Every protocol, ICMP included.
    All,
}

impl Protocol {
    /// Read a `protocol` value. The spelling is exact: `TCP` is not `tcp` (spec §6.2).
    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text {
            "tcp" => Some(Self::Tcp),
            "udp" => Some(Self::Udp),
            "all" => Some(Self::All),
            _ => None,
        }
    }

    /// Whether a rule with this protocol applies to a destination reached over `transport`.
    #[must_use]
    pub const fn admits(self, transport: Transport) -> bool {
        matches!(
            (self, transport),
            (Self::All, _) | (Self::Tcp, Transport::Tcp) | (Self::Udp, Transport::Udp)
        )
    }
}

/// The protocol of a destination being looked up (spec §6.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Transport {
    /// A TCP connection.
    Tcp,
    /// A UDP datagram.
    Udp,
    /// ICMP, which has no ports: a rule's port range is not consulted.
    Icmp,
}
