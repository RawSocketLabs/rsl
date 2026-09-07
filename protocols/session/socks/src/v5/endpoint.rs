use super::AddressType;
use bnb::bin;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

/// A SOCKS5 address and port, including its wire `ATYP` discriminant (RFC 1928 §4).
///
/// Domain names are raw bytes so decoding never loses non-UTF-8 input. Their one-octet length
/// prefix is derived during encoding; names longer than 255 bytes fail to encode.
//~ models rfc1928#4 part="address and port"
#[bin(big)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Endpoint {
    /// An IPv4 address and port.
    #[bin(magic = 0x01u8)]
    Ipv4 {
        /// The IPv4 address.
        address: Ipv4Addr,
        /// The network port.
        port: u16,
    },
    /// A length-prefixed domain name and port.
    #[bin(magic = 0x03u8)]
    Domain {
        /// The domain name bytes, without a trailing terminator.
        #[brw(count_prefix = u8)]
        name: Vec<u8>,
        /// The network port.
        port: u16,
    },
    /// An IPv6 address and port.
    #[bin(magic = 0x04u8)]
    Ipv6 {
        /// The IPv6 address.
        address: Ipv6Addr,
        /// The network port.
        port: u16,
    },
}

impl Endpoint {
    /// Constructs a domain endpoint without requiring UTF-8.
    #[must_use]
    pub fn domain(name: impl Into<Vec<u8>>, port: u16) -> Self {
        Self::Domain {
            name: name.into(),
            port,
        }
    }

    /// Returns the endpoint's address type.
    #[must_use]
    pub const fn address_type(&self) -> AddressType {
        match self {
            Self::Ipv4 { .. } => AddressType::Ipv4,
            Self::Domain { .. } => AddressType::Domain,
            Self::Ipv6 { .. } => AddressType::Ipv6,
        }
    }

    /// Returns the endpoint's port.
    #[must_use]
    pub const fn port(&self) -> u16 {
        match self {
            Self::Ipv4 { port, .. } | Self::Domain { port, .. } | Self::Ipv6 { port, .. } => *port,
        }
    }
}

impl From<SocketAddr> for Endpoint {
    fn from(value: SocketAddr) -> Self {
        match value.ip() {
            IpAddr::V4(address) => Self::Ipv4 {
                address,
                port: value.port(),
            },
            IpAddr::V6(address) => Self::Ipv6 {
                address,
                port: value.port(),
            },
        }
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn socket_addresses_keep_family_and_port() {
        let v4 = Endpoint::from(SocketAddr::from(([192, 0, 2, 1], 1080)));
        assert_eq!(v4.address_type(), AddressType::Ipv4);
        assert_eq!(v4.port(), 1080);

        let v6 = Endpoint::from(SocketAddr::from(([0x2001, 0xdb8, 0, 0, 0, 0, 0, 1], 443)));
        assert_eq!(v6.address_type(), AddressType::Ipv6);
        assert_eq!(v6.port(), 443);
    }
}
