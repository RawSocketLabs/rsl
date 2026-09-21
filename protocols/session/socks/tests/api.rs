//! Fast-tier configuration contracts and lossless application/wire conversions.
#![cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]

use socks::{
    Client, Destination, Server, Version,
    error::Error,
    server::policy::{Policy, ServerAuth},
    v5,
};
use std::{net::SocketAddr, time::Duration};

fn deny() -> Policy {
    Policy::new(ServerAuth::no_authentication(), |_| {
        panic!("construction must not run policy")
    })
}

#[test]
fn server_requires_an_explicit_nonempty_protocol_set_and_policy() {
    assert!(matches!(
        Server::builder().protocols([Version::V5]).build(),
        Err(Error::MissingPolicy)
    ));
    assert!(matches!(
        Server::builder().policy(deny()).build(),
        Err(Error::NoProtocols)
    ));
    assert!(matches!(
        Server::new(socks::server::ServerConfig::new([], deny())),
        Err(Error::NoProtocols)
    ));
    let server = Server::builder()
        .protocols([Version::V5, Version::V5])
        .policy(deny())
        .build()
        .unwrap();
    assert_eq!(
        server.protocols(),
        [Version::V5],
        "repeated versions are a set, not repeated handshakes"
    );
    let limits = socks::server::Limits {
        handshake: Duration::ZERO,
        ..Default::default()
    };
    assert!(matches!(
        Server::builder()
            .protocols([Version::V5])
            .policy(deny())
            .limits(limits)
            .build(),
        Err(Error::InvalidLimits)
    ));
}

#[test]
fn client_requires_explicit_protocol_valid_credentials_and_tcp_budget() {
    assert!(matches!(
        Client::builder().build(),
        Err(Error::MissingProtocol)
    ));
    for length in [0, 256] {
        assert!(v5::client::Config::username_password(vec![b'u'; length], b"p").is_err());
        assert!(v5::client::Config::username_password(b"u", vec![b'p'; length]).is_err());
    }
    let config = v5::client::Config::username_password(vec![b'u'; 255], vec![b'p'; 255]).unwrap();
    let client = Client::builder().protocol(config).build().unwrap();
    assert_eq!(client.version(), Version::V5);
    for timeout in [Duration::ZERO, Duration::MAX] {
        assert!(matches!(
            Client::builder()
                .protocol(v5::client::Config::no_authentication())
                .timeout(timeout)
                .build(),
            Err(Error::InvalidLimits)
        ));
    }
}

#[test]
fn neutral_destinations_preserve_wire_address_bytes_without_applying_dns_policy() {
    let ipv4: SocketAddr = "192.0.2.1:443".parse().unwrap();
    let ipv6: SocketAddr = "[2001:db8::1]:1080".parse().unwrap();
    for destination in [
        Destination::from(ipv4),
        Destination::from(ipv6),
        Destination::domain([0xff, 0, b'A'], 443),
        Destination::domain([], 0),
        Destination::domain(vec![b'x'; 256], 65535),
    ] {
        let wire: v5::Endpoint = destination.clone().into();
        assert_eq!(
            Destination::from(wire),
            destination,
            "conversion must not normalize or discard raw address data"
        );
    }
}
