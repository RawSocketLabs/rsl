//! Application destinations, independent of any SOCKS wire address registry.

use std::net::{IpAddr, SocketAddr};

/// An IP address or opaque domain name and port, without a wire discriminant.
/// Protocol-specific length and address-family constraints are checked by the
/// selected client before I/O. System DNS additionally requires UTF-8 without NUL.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Destination {
    /// A numeric IP destination.
    Ip {
        /// IPv4 or IPv6 address.
        address: IpAddr,
        /// Network port.
        port: u16,
    },
    /// An unresolved domain; no normalization or UTF-8 conversion is performed.
    Domain {
        /// Original domain bytes.
        name: Vec<u8>,
        /// Network port.
        port: u16,
    },
}

impl Destination {
    /// Construct an unresolved destination, retaining its exact bytes.
    #[must_use]
    pub fn domain(name: impl Into<Vec<u8>>, port: u16) -> Self {
        Self::Domain {
            name: name.into(),
            port,
        }
    }

    /// The requested port.
    #[must_use]
    pub const fn port(&self) -> u16 {
        match self {
            Self::Ip { port, .. } | Self::Domain { port, .. } => *port,
        }
    }

    /// A numeric destination without DNS, or `None` for a domain.
    #[must_use]
    pub fn socket_addr(&self) -> Option<SocketAddr> {
        match self {
            Self::Ip { address, port } => Some(SocketAddr::new(*address, *port)),
            Self::Domain { .. } => None,
        }
    }
}

impl From<SocketAddr> for Destination {
    /// SOCKS addresses carry no IPv6 scope ID or flow label.
    fn from(value: SocketAddr) -> Self {
        Self::Ip {
            address: value.ip(),
            port: value.port(),
        }
    }
}

impl From<crate::v5::Endpoint> for Destination {
    fn from(value: crate::v5::Endpoint) -> Self {
        use crate::v5::Endpoint;
        match value {
            Endpoint::Ipv4 { address, port } => Self::Ip {
                address: address.into(),
                port,
            },
            Endpoint::Ipv6 { address, port } => Self::Ip {
                address: address.into(),
                port,
            },
            Endpoint::Domain(domain) => Self::Domain {
                name: domain.name,
                port: domain.port,
            },
        }
    }
}
