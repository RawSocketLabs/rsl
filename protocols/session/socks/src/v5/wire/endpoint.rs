// --- Standard library ---
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

// --- Workspace dependencies ---
use bnb::bin;

// --- Internal modules ---
use super::{AddressType, Domain, DomainError};
#[cfg(any(feature = "blocking", feature = "tokio"))]
use crate::error::Error;

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
    Domain(#[brw(variable)] Domain),
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
    /// Construct a numeric endpoint from an IPv4, IPv6, or generic IP address and port.
    ///
    /// Performs no parsing or DNS lookup. Port zero is representable here; guided
    /// CONNECT operations reject it separately. Use [`Self::domain`] for domain names.
    ///
    /// ```
    /// use socks::v5::Endpoint;
    /// use std::net::{Ipv4Addr, Ipv6Addr};
    ///
    /// let ipv4 = Endpoint::addr(Ipv4Addr::LOCALHOST, 443);
    /// let ipv6 = Endpoint::addr(Ipv6Addr::LOCALHOST, 443);
    /// assert_eq!(ipv4.port(), ipv6.port());
    /// ```
    #[must_use]
    pub fn addr(address: impl Into<IpAddr>, port: u16) -> Self {
        Self::from(SocketAddr::new(address.into(), port))
    }

    /// Check domain construction constraints; numeric endpoints need no further checks.
    ///
    /// This is explicit validation, not a decode hook or DNS policy check. Port zero
    /// is allowed here; guided CONNECT operations validate the destination separately.
    ///
    /// # Errors
    /// Returns the domain's length error when its name is empty or longer than 255 bytes.
    pub fn validate_structure(&self) -> Result<(), DomainError> {
        match self {
            Self::Ipv4 { .. } | Self::Ipv6 { .. } => Ok(()),
            Self::Domain(domain) => domain.check_length(),
        }
    }

    /// Construct a domain endpoint with a 1–255-byte name.
    ///
    /// Checks only the wire length, not DNS syntax, UTF-8, or IDNA. Port zero is
    /// representable here; guided CONNECT operations reject it as a destination.
    /// Use [`Self::domain_raw`] to deliberately bypass length validation.
    ///
    /// # Errors
    /// Returns [`DomainError`] for an empty name or one longer than 255 bytes.
    ///
    /// ```
    /// use socks::v5::{DomainError, Endpoint};
    /// let endpoint = Endpoint::domain(b"example.com", 443)?;
    /// assert_eq!(endpoint.port(), 443);
    /// assert_eq!(Endpoint::domain([], 443), Err(DomainError::Empty));
    /// # Ok::<(), DomainError>(())
    /// ```
    pub fn domain(name: impl Into<Vec<u8>>, port: u16) -> Result<Self, DomainError> {
        Domain::try_new(name, port).map(Self::Domain)
    }

    /// Construct a raw domain endpoint without checking its name or port.
    ///
    /// Empty names remain encodable; names longer than 255 bytes fail encoding.
    /// Guided operations still validate raw values before use.
    #[must_use]
    pub fn domain_raw(name: impl Into<Vec<u8>>, port: u16) -> Self {
        Self::Domain(Domain {
            name: name.into(),
            port,
        })
    }

    /// Check structure and require a nonzero port for a guided CONNECT destination.
    ///
    /// This check is CONNECT-specific, not a general rule for reply endpoints or
    /// future BIND and UDP ASSOCIATE destinations.
    #[cfg(any(feature = "blocking", feature = "tokio"))]
    pub(crate) fn validate_destination(&self) -> Result<(), Error> {
        self.validate_structure()?;

        (self.port() != 0)
            .then_some(())
            .ok_or(Error::ZeroDestinationPort)
    }

    /// Returns the endpoint's address type.
    #[must_use]
    pub const fn address_type(&self) -> AddressType {
        match self {
            Self::Ipv4 { .. } => AddressType::Ipv4,
            Self::Domain(_) => AddressType::Domain,
            Self::Ipv6 { .. } => AddressType::Ipv6,
        }
    }

    /// Returns the endpoint's port.
    #[must_use]
    pub const fn port(&self) -> u16 {
        match self {
            Self::Ipv4 { port, .. } | Self::Ipv6 { port, .. } => *port,
            Self::Domain(domain) => domain.port,
        }
    }
}

impl From<Domain> for Endpoint {
    /// Wrap the payload without revalidating it; raw values remain representable.
    fn from(domain: Domain) -> Self {
        Self::Domain(domain)
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

impl From<crate::Destination> for Endpoint {
    fn from(value: crate::Destination) -> Self {
        match value {
            crate::Destination::Ip {
                address: IpAddr::V4(address),
                port,
            } => Self::Ipv4 { address, port },
            crate::Destination::Ip {
                address: IpAddr::V6(address),
                port,
            } => Self::Ipv6 { address, port },
            crate::Destination::Domain { name, port } => Self::Domain(Domain { name, port }),
        }
    }
}

#[cfg(test)]
mod unit {
    // --- Internal modules ---
    use super::*;

    #[cfg(any(feature = "blocking", feature = "tokio"))]
    #[test]
    fn connect_checks_ports_without_restricting_general_endpoints() {
        for port in [0, 1, u16::MAX] {
            for endpoint in [
                Endpoint::Ipv4 {
                    address: Ipv4Addr::LOCALHOST,
                    port,
                },
                Endpoint::Ipv6 {
                    address: Ipv6Addr::LOCALHOST,
                    port,
                },
                Endpoint::domain(b"example.com", port).unwrap(),
            ] {
                assert!(endpoint.validate_structure().is_ok());
                let result = endpoint.validate_destination();
                if port == 0 {
                    assert!(matches!(result, Err(Error::ZeroDestinationPort)));
                } else {
                    assert!(result.is_ok(), "{endpoint:?}: {result:?}");
                }
            }
        }
        // Length errors precede the operation-specific port check, even for raw values.
        for (length, expected) in [
            (0, DomainError::Empty),
            (256, DomainError::TooLong { length: 256 }),
        ] {
            assert!(matches!(
                Endpoint::domain_raw(vec![b'x'; length], 0).validate_destination(),
                Err(Error::InvalidDomain(error)) if error == expected
            ));
        }
    }

    #[test]
    fn numeric_constructor_preserves_addresses_and_port_boundaries() {
        for port in [0, 443, u16::MAX] {
            let ipv4 = Ipv4Addr::new(192, 0, 2, 1);
            let ipv6 = Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1);
            assert_eq!(
                Endpoint::addr(ipv4, port),
                Endpoint::Ipv4 {
                    address: ipv4,
                    port
                }
            );
            assert_eq!(
                Endpoint::addr(ipv6, port),
                Endpoint::Ipv6 {
                    address: ipv6,
                    port
                }
            );
            assert_eq!(
                Endpoint::addr(IpAddr::V4(ipv4), port),
                Endpoint::addr(ipv4, port)
            );
            assert_eq!(
                Endpoint::addr(IpAddr::V6(ipv6), port),
                Endpoint::addr(ipv6, port)
            );
        }
    }

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
