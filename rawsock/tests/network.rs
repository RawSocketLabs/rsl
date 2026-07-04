//! The privileged L3 backend. The capability-consistency check runs everywhere (CI included);
//! the real raw send is gated on `CAP_NET_RAW` and skips cleanly when unprivileged.
#![cfg(all(target_os = "linux", feature = "network"))]

use rawsock::{Layer, NetworkSocket, OpenError, RawIo, capabilities};
use std::net::Ipv4Addr;

#[test]
fn open_is_consistent_with_the_capability_probe() {
    let can = capabilities().network;
    match NetworkSocket::open() {
        Ok(sock) => {
            assert!(
                can,
                "opened an L3 socket the capability probe reported unavailable"
            );
            assert!(matches!(sock.layer(), Layer::Network));
        }
        Err(OpenError::PermissionDenied) => {
            assert!(!can, "probe reported L3 available but open was denied");
        }
        Err(e) => panic!("unexpected open error: {e}"),
    }
}

/// A minimal 28-byte IPv4 + UDP datagram to 127.0.0.1:9 (discard), checksums left 0 for the
/// kernel/receiver. Src and dst are loopback so it never leaves the host.
fn loopback_datagram() -> Vec<u8> {
    let mut d = vec![
        0x45, 0x00, 0x00, 0x1c, // ver/IHL, DSCP, total_length 28
        0x00, 0x00, 0x40, 0x00, // id, flags (DF)
        0x40, 0x11, 0x00, 0x00, // TTL 64, proto 17 (UDP), checksum 0 (kernel fills)
    ];
    d.extend_from_slice(&Ipv4Addr::LOCALHOST.octets()); // src
    d.extend_from_slice(&Ipv4Addr::LOCALHOST.octets()); // dst
    d.extend_from_slice(&[0x00, 0x09, 0x00, 0x09, 0x00, 0x08, 0x00, 0x00]); // UDP 9→9, len 8
    d
}

#[test]
fn privileged_raw_send_to_loopback() {
    if !capabilities().network {
        return; // needs CAP_NET_RAW — skipped on an unprivileged runner (CI)
    }
    let mut sock = NetworkSocket::open().unwrap();
    sock.connect(Ipv4Addr::LOCALHOST).unwrap();
    let datagram = loopback_datagram();
    let n = sock.send_raw(&datagram).unwrap();
    assert_eq!(n, datagram.len());
}
