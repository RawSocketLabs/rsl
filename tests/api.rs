//! Public-API surface: the high-level types are reachable, construct without
//! surprise, and keep their documented shapes. These guard against accidental
//! breaking changes to the API a downstream consumer depends on — they assert
//! shape, not network behavior.

use std::net::SocketAddr;
use std::time::Duration;

use socks::client::{Client, TargetAddr};
use socks::error::SocksError;
use socks::server::{
    Server, DEFAULT_BIND_TIMEOUT, DEFAULT_HANDSHAKE_TIMEOUT, DEFAULT_MAX_CONNECTIONS,
};

fn loopback() -> SocketAddr {
    "127.0.0.1:1080".parse().unwrap()
}

#[test]
fn client_constructors_are_reachable() {
    let proxy = loopback();
    let _ = Client::new(proxy);
    let _ = Client::with_user_pass(proxy, "user", "pass").expect("valid credentials");
    let _ = Client::with_auth(proxy, vec![]);
    assert_eq!(Client::new(proxy).proxy, proxy);
}

#[test]
fn oversized_credentials_are_rejected_at_construction() {
    let long = "x".repeat(256);
    let result = Client::with_user_pass(loopback(), &long, "pass");
    assert!(matches!(result, Err(SocksError::Validation(_))));
}

#[test]
fn server_builder_chains_and_exposes_defaults() {
    // The builder methods are all `self -> Self`, so they compose in any order.
    let server = Server::bind("127.0.0.1:0")
        .unwrap()
        .with_handshake_timeout(Some(Duration::from_secs(10)))
        .with_bind_timeout(None)
        .with_max_connections(8);
    assert!(server.local_addr().is_ok());

    // The documented default constants are public and sane.
    assert_eq!(DEFAULT_HANDSHAKE_TIMEOUT, Duration::from_secs(30));
    assert_eq!(DEFAULT_BIND_TIMEOUT, Duration::from_secs(120));
    assert_eq!(DEFAULT_MAX_CONNECTIONS, 1024);
}

#[test]
fn max_connections_is_floored_at_one() {
    // Documented invariant: a zero cap is clamped up to 1, never left at 0.
    let server = Server::bind("127.0.0.1:0").unwrap().with_max_connections(0);
    assert!(server.local_addr().is_ok());
}

#[test]
fn target_addr_conversions() {
    let socket = loopback();
    assert_eq!(TargetAddr::from(socket), TargetAddr::Ip(socket));
    assert_eq!(
        TargetAddr::from(("example.com", 443)),
        TargetAddr::Domain("example.com".to_string(), 443)
    );
    assert_eq!(
        TargetAddr::from(("example.com".to_string(), 80)),
        TargetAddr::Domain("example.com".to_string(), 80)
    );
}

#[test]
fn error_variants_display() {
    // The error type is the crate's public failure currency; its variants must
    // render non-empty, human-readable messages.
    for err in [
        SocksError::NoAcceptableMethods,
        SocksError::AuthenticationFailed,
        SocksError::UnsupportedVersion(4),
        SocksError::Validation("bad".into()),
        SocksError::NotSupported("thing".into()),
    ] {
        assert!(!err.to_string().is_empty(), "{err:?} rendered empty");
    }
}
