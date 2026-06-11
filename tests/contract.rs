#![cfg(feature = "v5")]
//! Wire-format contract: fixed byte vectors taken straight from RFC 1928 /
//! RFC 1929 must parse, through the public API, into the expected typed values.
//!
//! The in-source unit tests cover the serialize direction; these pin the parse
//! direction against golden bytes so a refactor cannot silently change how the
//! crate reads the wire. Unassigned code points are asserted to pass through as
//! `Custom`, not to be rejected — the crate observes the wire, it does not
//! police it.

use socks::v5::{
    Address, Command, Identifier, Method, Offer, Reply, Request, Response, UserPassRequest,
    UserPassResponse, UserPassStatus,
};

#[test]
fn identifier_method_selection() {
    // VER=5, NMETHODS=2, METHODS=[NoAuth, Username/Password] (RFC 1928 §3).
    let mut bytes = [5u8, 2, 0x00, 0x02].as_slice();
    let id = Identifier::read_from(&mut bytes).expect("parses");
    assert_eq!(id.version, 5);
    assert_eq!(id.methods, vec![Method::NoAuth, Method::Plain]);
}

#[test]
fn offer_method_reply() {
    // VER=5, METHOD=NoAuth (RFC 1928 §3).
    let offer = Offer::read_from(&mut [5u8, 0x00].as_slice()).expect("parses");
    assert_eq!(offer.version, 5);
    assert_eq!(offer.method, Method::NoAuth);
}

#[test]
fn request_connect_ipv4() {
    // VER=5, CMD=CONNECT, RSV=0, ATYP=IPv4, 127.0.0.1, port 443 (RFC 1928 §4).
    let mut bytes = [5u8, 1, 0, 1, 127, 0, 0, 1, 0x01, 0xBB].as_slice();
    let request = Request::read_from(&mut bytes).expect("parses");
    assert_eq!(request.command, Command::Connect);
    assert_eq!(request.reserved, 0);
    assert_eq!(request.dest_addr.to_socket_string(request.dest_port), "127.0.0.1:443");
}

#[test]
fn request_connect_domain() {
    // VER=5, CMD=CONNECT, RSV=0, ATYP=DOMAIN, len=11, "example.com", port 80.
    let mut bytes = Vec::from([5u8, 1, 0, 3, 11]);
    bytes.extend_from_slice(b"example.com");
    bytes.extend_from_slice(&80u16.to_be_bytes());
    let request = Request::read_from(&mut bytes.as_slice()).expect("parses");
    assert!(matches!(request.dest_addr, Address::Domain(_)));
    assert_eq!(request.dest_addr.to_socket_string(request.dest_port), "example.com:80");
}

#[test]
fn reply_succeeded() {
    // VER=5, REP=0, RSV=0, ATYP=IPv4, 127.0.0.1, port 1080 (RFC 1928 §6).
    let mut bytes = [5u8, 0, 0, 1, 127, 0, 0, 1, 0x04, 0x38].as_slice();
    let reply = Reply::read_from(&mut bytes).expect("parses");
    assert_eq!(reply.reply, Response::Succeeded);
    assert_eq!(reply.bind_port, 1080);
}

#[test]
fn reply_unassigned_code_passes_through() {
    // REP=0x09 is unassigned by RFC 1928; the crate carries it as `Custom`
    // rather than failing to parse — observing the wire, not policing it.
    let mut bytes = [5u8, 0x09, 0, 1, 0, 0, 0, 0, 0, 0].as_slice();
    let reply = Reply::read_from(&mut bytes).expect("parses");
    assert_eq!(reply.reply, Response::Custom(0x09));
}

#[test]
fn user_pass_request_credentials() {
    // VER=1, ULEN=4, "user", PLEN=7, "hunter2" (RFC 1929 §2).
    let mut bytes = Vec::from([1u8, 4]);
    bytes.extend_from_slice(b"user");
    bytes.push(7);
    bytes.extend_from_slice(b"hunter2");
    let request = UserPassRequest::read_from(&mut bytes.as_slice()).expect("parses");
    assert_eq!(request.version, 1);
    assert_eq!(request.username, b"user");
    assert_eq!(request.password, b"hunter2");
}

#[test]
fn user_pass_response_status() {
    // VER=1, STATUS=0 (success); any non-zero status is failure (RFC 1929 §2).
    let ok = UserPassResponse::read_from(&mut [1u8, 0].as_slice()).expect("parses");
    assert_eq!(ok.status, UserPassStatus::Success);

    let denied = UserPassResponse::read_from(&mut [1u8, 1].as_slice()).expect("parses");
    assert_eq!(denied.status, UserPassStatus::Custom(0x01));
}
