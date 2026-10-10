//! Smoke test: the fastest "is it alive" check — one byte round-trips through a
//! real client and server.

mod common;

use common::{client, spawn_server};

#[test]
fn one_byte_uploads_and_downloads() {
    let (addr, store) = spawn_server();
    client(addr).put("b", b"!").unwrap();
    assert_eq!(store.get("b"), Some(b"!".to_vec()));
    assert_eq!(client(addr).get("b").unwrap(), b"!");
}
