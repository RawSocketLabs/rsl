//! Regression guards for specific behaviors that are easy to break silently.

mod common;

use common::{client, spawn_server_with, TEST_TIMEOUT};
use tftp::client::RawClient;
use tftp::error::TftpError;
use tftp::server::MemoryStore;
use tftp::wire::{Packet, Request, TransferMode};

/// The server must answer a request from a freshly-allocated TID (a different
/// UDP port than the listen socket), so concurrent transfers don't collide and
/// the client can pin the server's TID (RFC 1350 §2, §4).
//~ verifies rfc1350#4/should.45bc6d
#[test]
fn server_answers_from_a_fresh_tid() {
    let (addr, _store) = spawn_server_with(MemoryStore::new().with_file("f", b"hi".to_vec()));

    let raw = RawClient::bind(addr).unwrap();
    raw.set_read_timeout(Some(TEST_TIMEOUT)).unwrap();
    raw.send(&Packet::Request(Request::read("f", TransferMode::Octet)))
        .unwrap();

    let (packet, src) = raw.recv().unwrap();
    assert_ne!(
        src.port(),
        addr.port(),
        "the server must reply from a new TID, not the listen port"
    );
    assert!(matches!(packet, Packet::Data(_)), "first reply should be DATA");
}

/// A datagram from a source other than the locked TID must be answered with an
/// ERROR(5) and must not disturb the running transfer (RFC 1350 §4). Here a
/// stray sender pokes the server's transfer socket and should get an ERROR back.
//~ verifies rfc1350#4/should.06d78b
#[test]
fn stray_sender_gets_unknown_tid_error() {
    let (addr, _store) = spawn_server_with(MemoryStore::new().with_file("f", vec![0u8; 2048]));

    // Start a transfer and learn the server's TID from its first DATA.
    let legit = RawClient::bind(addr).unwrap();
    legit.set_read_timeout(Some(TEST_TIMEOUT)).unwrap();
    legit
        .send(&Packet::Request(Request::read("f", TransferMode::Octet)))
        .unwrap();
    let (_data, server_tid) = legit.recv().unwrap();

    // A different socket pokes the server's transfer TID with an ACK.
    let stray = RawClient::bind(server_tid).unwrap();
    stray.set_read_timeout(Some(TEST_TIMEOUT)).unwrap();
    stray
        .send(&Packet::Ack(tftp::wire::Ack::new(99)))
        .unwrap();

    let (packet, _) = stray.recv().expect("stray should receive an ERROR");
    match packet {
        Packet::Error(err) => {
            assert_eq!(err.code, tftp::wire::ErrorCode::UnknownTransferId)
        }
        other => panic!("expected ERROR(5), got {:?}", other.opcode()),
    }
}

/// An empty filename is rejected locally, before any socket I/O.
#[test]
fn empty_filename_is_rejected_before_io() {
    // No server is running on this port; the call must fail at validation, not
    // by timing out on the network.
    let err = client("127.0.0.1:9".parse().unwrap())
        .get("")
        .expect_err("empty filename must be rejected");
    assert!(matches!(err, TftpError::Validation(_)), "got {err:?}");
}

/// A file whose size is an exact multiple of the block size must transfer a
/// trailing empty DATA block and complete cleanly.
#[test]
fn exact_block_multiple_terminates_with_empty_block() {
    let payload = vec![0xCDu8; 1024]; // exactly two 512-byte blocks
    let (addr, _store) = spawn_server_with(MemoryStore::new().with_file("m", payload.clone()));
    assert_eq!(client(addr).get("m").unwrap(), payload);
}
