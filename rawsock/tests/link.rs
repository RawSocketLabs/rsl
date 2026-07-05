//! The privileged L2 (`AF_PACKET`) backend. The capability-consistency check runs everywhere
//! (CI included); the real frame send is gated on `CAP_NET_RAW` and skips cleanly unprivileged.
#![cfg(all(target_os = "linux", feature = "link"))]

use rawsock::{Layer, LinkSocket, OpenError, RawIo, capabilities};

#[test]
fn open_is_consistent_with_the_capability_probe() {
    // "lo" always exists, so open() succeeds iff the host allows an AF_PACKET socket.
    let can = capabilities().link;
    match LinkSocket::open("lo") {
        Ok(sock) => {
            assert!(
                can,
                "opened an L2 socket the capability probe reported unavailable"
            );
            assert!(matches!(sock.layer(), Layer::Link));
        }
        Err(OpenError::PermissionDenied) => {
            assert!(!can, "probe reported L2 available but open was denied");
        }
        Err(e) => panic!("unexpected open error: {e}"),
    }
}

#[test]
fn open_of_an_unknown_interface_is_a_clean_error() {
    if !capabilities().link {
        return; // the socket open itself is denied first when unprivileged
    }
    // A bogus interface name — name_to_index fails, and it must be an error, not a panic.
    assert!(LinkSocket::open("definitely-not-an-iface-xyz").is_err());
}

#[test]
fn privileged_frame_send_on_loopback() {
    if !capabilities().link {
        return; // needs CAP_NET_RAW — skipped on an unprivileged runner (CI)
    }
    let mut sock = LinkSocket::open("lo").unwrap();
    // A minimal 60-byte frame (the Ethernet minimum). On loopback this may be accepted or
    // rejected depending on the host; either way it must not panic.
    let frame = [0u8; 60];
    let _ = sock.send_raw(&frame);
}
