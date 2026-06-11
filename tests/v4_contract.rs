#![cfg(feature = "v4")]
//! Wire-format contract: fixed byte vectors for the SOCKS4 / 4A messages,
//! exercised in both directions (build → bytes, bytes → parse). These vectors
//! are taken straight from the 1992 SOCKS4 memo and its 4A extension, so a
//! regression in the codec shows up as a vector mismatch rather than a subtle
//! behavioral drift.

use std::net::Ipv4Addr;

use binrw::{io::Cursor, BinWrite, NullString};
use socks::v4::{Command, Reply, ReplyCode, Request};

/// A SOCKS4 CONNECT request: VN=4, CD=1, port 23 (telnet), 66.102.7.99,
/// userid "Fred", NUL — the canonical worked example from the memo.
const CONNECT_FRED: &[u8] = &[
    4, 1, // VN, CD
    0x00, 0x17, // DSTPORT = 23
    66, 102, 7, 99, // DSTIP
    b'F', b'r', b'e', b'd', 0, // USERID + NUL
];

#[test]
fn connect_request_encodes_to_the_memo_vector() {
    let request = Request {
        version: 4,
        command: Command::Connect,
        dest_port: 23,
        dest_ip: Ipv4Addr::new(66, 102, 7, 99),
        userid: NullString::from("Fred"),
        #[cfg(feature = "v4a")]
        domain: None,
    };
    let mut cursor = Cursor::new(Vec::new());
    request.write(&mut cursor).unwrap();
    assert_eq!(cursor.into_inner(), CONNECT_FRED);
}

#[test]
fn connect_request_parses_from_the_memo_vector() {
    let mut reader = CONNECT_FRED;
    let request = Request::read_from(&mut reader).unwrap();
    assert_eq!(request.version, 4);
    assert_eq!(request.command, Command::Connect);
    assert_eq!(request.dest_port, 23);
    assert_eq!(request.dest_ip, Ipv4Addr::new(66, 102, 7, 99));
    assert_eq!(request.userid.to_string(), "Fred");
    assert!(reader.is_empty(), "the message must be exactly framed");
}

#[test]
fn reply_granted_vector() {
    // A granted reply: VN=0, CD=90, the rest zero.
    let bytes = &[0u8, 90, 0, 0, 0, 0, 0, 0];
    let reply = Reply::read_from(&mut &bytes[..]).unwrap();
    assert_eq!(reply.version, 0);
    assert_eq!(reply.code, ReplyCode::Granted);

    let mut cursor = Cursor::new(Vec::new());
    reply.write(&mut cursor).unwrap();
    assert_eq!(cursor.into_inner(), bytes);
}

#[test]
fn reply_codes_map_to_named_variants() {
    for (cd, expected) in [
        (90u8, ReplyCode::Granted),
        (91, ReplyCode::Rejected),
        (92, ReplyCode::IdentdUnreachable),
        (93, ReplyCode::IdentdMismatch),
    ] {
        let bytes = [0u8, cd, 0, 0, 0, 0, 0, 0];
        assert_eq!(Reply::read_from(&mut &bytes[..]).unwrap().code, expected);
    }
}

#[cfg(feature = "v4a")]
#[test]
fn socks4a_connect_request_vector() {
    // VN=4, CD=1, port 80, DSTIP=0.0.0.1 marker, empty userid + NUL,
    // "www.example.com" + NUL.
    let mut expected = vec![4u8, 1, 0x00, 0x50, 0, 0, 0, 1, 0];
    expected.extend_from_slice(b"www.example.com\0");

    let request = Request {
        version: 4,
        command: Command::Connect,
        dest_port: 80,
        dest_ip: Ipv4Addr::new(0, 0, 0, 1),
        userid: NullString::default(),
        domain: Some(NullString::from("www.example.com")),
    };
    let mut cursor = Cursor::new(Vec::new());
    request.write(&mut cursor).unwrap();
    assert_eq!(cursor.into_inner(), expected);

    let parsed = Request::read_from(&mut expected.as_slice()).unwrap();
    assert_eq!(
        parsed.domain.as_ref().map(|d| d.to_string()),
        Some("www.example.com".to_string())
    );
}
