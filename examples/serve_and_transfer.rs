//! A self-contained demo: start an in-memory TFTP server on loopback, then use
//! the client to upload a file and download it back.
//!
//! Run with: `cargo run -p tftp --example serve_and_transfer`

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use tftp::client::Client;
use tftp::server::{MemoryStore, Server};

fn main() -> Result<(), tftp::error::TftpError> {
    // A memory-backed server, preloaded with one file, serving on an ephemeral
    // loopback port in a background thread.
    let store = Arc::new(MemoryStore::new().with_file("motd.txt", b"hello from tftp\n"));
    let server = Server::bind("127.0.0.1:0", Arc::clone(&store) as Arc<_>)?
        .with_timeout(Duration::from_secs(1));
    let addr = server.local_addr()?;
    thread::spawn(move || {
        let _ = server.serve();
    });

    let client = Client::new(addr);

    // Download the preloaded file.
    let motd = client.get("motd.txt")?;
    println!("downloaded motd.txt: {:?}", String::from_utf8_lossy(&motd));

    // Upload a new file, then read it back through the client.
    client.put("greeting.txt", b"uploaded over UDP\n")?;
    let echoed = client.get("greeting.txt")?;
    println!(
        "round-tripped greeting.txt: {:?}",
        String::from_utf8_lossy(&echoed)
    );

    assert_eq!(echoed, b"uploaded over UDP\n");
    println!("done.");
    Ok(())
}
