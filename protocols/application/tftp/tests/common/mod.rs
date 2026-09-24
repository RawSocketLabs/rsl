//! Shared helpers for the TFTP test binaries.
#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use tftp::client::Client;
use tftp::server::{MemoryStore, Server};

/// A short transfer timeout so a stalled test fails fast rather than hanging.
pub const TEST_TIMEOUT: Duration = Duration::from_millis(500);

/// Spawns a memory-backed TFTP server on an ephemeral loopback port, serving in
/// a background thread, and returns its address and the shared store (so a test
/// can preload downloads or read back uploads).
pub fn spawn_server() -> (SocketAddr, Arc<MemoryStore>) {
    spawn_server_with(MemoryStore::new())
}

/// Like [`spawn_server`] but with a preconfigured store.
pub fn spawn_server_with(store: MemoryStore) -> (SocketAddr, Arc<MemoryStore>) {
    let store = Arc::new(store);
    let server = Server::bind("127.0.0.1:0", Arc::clone(&store) as Arc<_>)
        .unwrap()
        .with_timeout(TEST_TIMEOUT);
    let addr = server.local_addr().unwrap();
    thread::spawn(move || {
        let _ = server.serve();
    });
    (addr, store)
}

/// A client pointed at `server` with the test timeout applied.
pub fn client(server: SocketAddr) -> Client {
    Client::new(server).with_timeout(TEST_TIMEOUT)
}
