//! End-to-end transfers: a real client through real UDP loopback to a real
//! server, exercising the full lock-step state machine in both directions over
//! a range of file sizes, modes, and (with the `options` feature) negotiation.

mod common;

use common::{client, spawn_server, spawn_server_with};
use tftp::error::TftpError;
use tftp::server::MemoryStore;
use tftp::wire::{ErrorCode, TransferMode};

/// Sizes chosen to cover the block-boundary edge cases: empty, sub-block,
/// exactly one block, exactly several blocks (forces a trailing empty block),
/// and a few blocks with a short final one.
const SIZES: &[usize] = &[0, 1, 511, 512, 513, 1024, 1500, 4096];

#[test]
fn download_round_trips_every_size() {
    for &size in SIZES {
        let payload: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
        let (addr, _store) = spawn_server_with(MemoryStore::new().with_file("f", payload.clone()));

        let got = client(addr).get("f").unwrap_or_else(|e| panic!("size {size}: {e}"));
        assert_eq!(got, payload, "download mismatch at size {size}");
    }
}

#[test]
fn upload_round_trips_every_size() {
    for &size in SIZES {
        let payload: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
        let (addr, store) = spawn_server();

        client(addr).put("up", &payload).unwrap_or_else(|e| panic!("size {size}: {e}"));
        assert_eq!(store.get("up"), Some(payload), "upload mismatch at size {size}");
    }
}

#[test]
fn download_missing_file_reports_peer_error() {
    let (addr, _store) = spawn_server();
    let err = client(addr).get("absent").expect_err("missing file must error");
    assert!(
        matches!(err, TftpError::Peer { code: ErrorCode::FileNotFound, .. }),
        "got {err:?}"
    );
}

#[test]
fn netascii_round_trips_text_with_newlines() {
    let text = b"first line\nsecond line\nthird\n".to_vec();
    let (addr, store) = spawn_server();

    // Upload in netascii: on the wire the newlines are CR-LF, but the server
    // translates back to host text before storing.
    client(addr)
        .mode(TransferMode::NetAscii)
        .put("notes.txt", &text)
        .unwrap();
    assert_eq!(store.get("notes.txt"), Some(text.clone()));

    // And a netascii download translates back to the same host text.
    let got = client(addr).mode(TransferMode::NetAscii).get("notes.txt").unwrap();
    assert_eq!(got, text);
}

//~ verifies rfc1350#5/must.30d6fb
#[test]
fn upload_then_download_is_identity() {
    let payload: Vec<u8> = (0..3000).map(|i| (i * 7 % 256) as u8).collect();
    let (addr, _store) = spawn_server();

    client(addr).put("blob", &payload).unwrap();
    let got = client(addr).get("blob").unwrap();
    assert_eq!(got, payload);
}

#[cfg(feature = "options")]
#[test]
fn download_with_negotiated_blksize() {
    // A large file with a small negotiated block size exercises the OACK path
    // and many blocks.
    let payload: Vec<u8> = (0..5000).map(|i| (i % 256) as u8).collect();
    let (addr, _store) = spawn_server_with(MemoryStore::new().with_file("big", payload.clone()));

    let got = client(addr).block_size(256).get("big").unwrap();
    assert_eq!(got, payload);
}

#[cfg(feature = "options")]
#[test]
fn upload_with_negotiated_blksize_and_tsize() {
    let payload: Vec<u8> = (0..5000).map(|i| (i % 256) as u8).collect();
    let (addr, store) = spawn_server();

    client(addr)
        .block_size(1024)
        .request_tsize(true)
        .put("big", &payload)
        .unwrap();
    assert_eq!(store.get("big"), Some(payload));
}
