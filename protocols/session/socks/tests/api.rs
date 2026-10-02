//! Fast-tier configuration contracts and lossless application/wire conversions.

use socks::{Client, Destination, v5};
#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
use socks::{
    Server,
    server::policy::{Policy, ServerAuth},
};
#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
use socks::{Version, error::Error};
use std::net::SocketAddr;
#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
use std::time::Duration;

#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
fn deny() -> Policy {
    Policy::new(ServerAuth::no_authentication(), |_| {
        panic!("construction must not run policy")
    })
}

#[cfg(any(feature = "blocking", feature = "tokio", feature = "mio"))]
macro_rules! server_construction_contract {
    ($name:ident, $backend:ident) => {
        #[test]
        fn $name() {
            fn configured<B>(builder: socks::server::Builder<B>) -> socks::server::Builder<B> {
                builder.protocols([Version::V5, Version::V5]).policy(deny())
            }

            assert!(matches!(
                Server::configure()
                    .protocols([Version::V5])
                    .$backend()
                    .build(),
                Err(Error::MissingPolicy)
            ));
            assert!(matches!(
                Server::configure().$backend().policy(deny()).build(),
                Err(Error::NoProtocols)
            ));
            assert!(matches!(
                socks::server::$backend::Server::new(socks::server::ServerConfig::new([], deny())),
                Err(Error::NoProtocols)
            ));
            let builder: socks::server::$backend::Builder =
                configured(Server::configure().$backend());
            let server = builder.build().unwrap();
            assert_eq!(
                server.protocols(),
                [Version::V5],
                "repeated versions are a set, not repeated handshakes"
            );
            let limits = socks::server::Limits {
                handshake: Duration::ZERO,
                ..Default::default()
            };
            let mut cfg = socks::server::ServerConfig::new([Version::V5], deny());
            cfg.limits = limits;
            assert!(matches!(
                socks::server::$backend::Server::new(cfg),
                Err(Error::InvalidLimits)
            ));
            assert!(matches!(
                Server::configure()
                    .protocols([Version::V5])
                    .policy(deny())
                    .limits(limits)
                    .$backend()
                    .build(),
                Err(Error::InvalidLimits)
            ));
        }
    };
}

#[cfg(feature = "blocking")]
server_construction_contract!(blocking_server_validates_configuration_without_io, blocking);
#[cfg(feature = "mio")]
server_construction_contract!(mio_server_validates_configuration_without_io, mio);
#[cfg(feature = "tokio")]
server_construction_contract!(tokio_server_validates_configuration_without_io, tokio);

#[test]
fn client_configuration_requires_valid_credentials_without_a_backend() {
    for length in [0, 256] {
        assert!(v5::client::Config::username_password(vec![b'u'; length], b"p").is_err());
        assert!(v5::client::Config::username_password(b"u", vec![b'p'; length]).is_err());
    }
    let cfg = v5::client::Config::username_password(vec![b'u'; 255], vec![b'p'; 255]).unwrap();
    let _builder = Client::configure(cfg);
    let configuration = socks::client::Configuration::V5(v5::client::Config::no_authentication());
    let _builder = Client::configure(configuration);
}

#[test]
fn neutral_destinations_preserve_wire_address_bytes_without_applying_dns_policy() {
    let ipv4: SocketAddr = "192.0.2.1:443".parse().unwrap();
    let ipv6: SocketAddr = "[2001:db8::1]:1080".parse().unwrap();
    for dest in [
        Destination::from(ipv4),
        Destination::from(ipv6),
        Destination::domain([0xff, 0, b'A'], 443),
        Destination::domain([], 0),
        Destination::domain(vec![b'x'; 256], 65535),
    ] {
        let wire: v5::Endpoint = dest.clone().into();
        assert_eq!(
            Destination::from(wire),
            dest,
            "conversion must not normalize or discard raw address data"
        );
    }
}

#[cfg(all(feature = "blocking", feature = "tokio", feature = "mio"))]
#[test]
fn generic_builder_helpers_preserve_backend_types_and_aliases() {
    fn with_budget<B>(builder: socks::client::Builder<B>) -> socks::client::Builder<B> {
        builder.timeout(Duration::from_secs(5))
    }

    let blocking: socks::client::Builder<socks::client::Blocking> =
        Client::configure(v5::client::Config::no_authentication()).blocking();
    let blocking: socks::client::blocking::Builder = with_budget(blocking);
    let _: socks::client::blocking::Client = blocking.build().unwrap();

    let mio: socks::client::Builder<socks::client::Mio> =
        Client::configure(v5::client::Config::no_authentication()).mio();
    let mio: socks::client::mio::Builder = with_budget(mio);
    let _: socks::client::mio::Client = mio.build().unwrap();

    let tokio: socks::client::Builder<socks::client::Tokio> =
        Client::configure(v5::client::Config::no_authentication()).tokio();
    let tokio: socks::client::tokio::Builder = with_budget(tokio);
    let _: socks::client::tokio::Client = tokio.build().unwrap();
}

#[cfg(all(feature = "blocking", feature = "tokio", feature = "mio"))]
#[test]
fn backend_selection_retains_shared_timeout_validation() {
    let blocking: socks::client::blocking::Client =
        Client::configure(v5::client::Config::no_authentication())
            .blocking()
            .build()
            .unwrap();
    let asynchronous: socks::client::tokio::Client =
        Client::configure(v5::client::Config::no_authentication())
            .tokio()
            .build()
            .unwrap();
    let readiness: socks::client::mio::Client =
        Client::configure(v5::client::Config::no_authentication())
            .mio()
            .build()
            .unwrap();
    assert_eq!(blocking.version(), Version::V5);
    assert_eq!(asynchronous.version(), Version::V5);
    assert_eq!(readiness.version(), Version::V5);
    for timeout in [Duration::ZERO, Duration::MAX] {
        assert!(matches!(
            Client::configure(v5::client::Config::no_authentication())
                .timeout(timeout)
                .blocking()
                .build(),
            Err(Error::InvalidLimits)
        ));
        assert!(matches!(
            Client::configure(v5::client::Config::no_authentication())
                .blocking()
                .timeout(timeout)
                .build(),
            Err(Error::InvalidLimits)
        ));
        assert!(matches!(
            Client::configure(v5::client::Config::no_authentication())
                .timeout(timeout)
                .tokio()
                .build(),
            Err(Error::InvalidLimits)
        ));
        assert!(matches!(
            Client::configure(v5::client::Config::no_authentication())
                .tokio()
                .timeout(timeout)
                .build(),
            Err(Error::InvalidLimits)
        ));
        assert!(matches!(
            Client::configure(v5::client::Config::no_authentication())
                .timeout(timeout)
                .mio()
                .build(),
            Err(Error::InvalidLimits)
        ));
        assert!(matches!(
            Client::configure(v5::client::Config::no_authentication())
                .mio()
                .timeout(timeout)
                .build(),
            Err(Error::InvalidLimits)
        ));
    }
}
