//! **mock_datagram** — testing `MessageDatagram` code with the `mock` feature, no real socket.
//!
//! `net`'s [`DatagramSocket`](bnb::DatagramSocket) trait is *sealed* (only `bnb` implements it), so
//! to unit-test datagram logic you reach for [`MockDatagramSocket`](bnb::MockDatagramSocket) (the
//! `mock` feature) instead of binding a real `UdpSocket`. Write your handler **generic over the
//! socket** — sealing forbids *implementing* the trait, not *using it as a bound* — so the same
//! code runs over a real `UdpSocket` in production and the mock in tests. Put `features = ["mock"]`
//! in your crate's `[dev-dependencies]`.
//!
//! Run with: `cargo run -p bitsandbytes --example mock_datagram --features mock`

use bnb::{DatagramSocket, MessageDatagram, MockDatagramSocket, bin};
use std::net::SocketAddr;

#[bin(big)]
#[derive(Debug, PartialEq, Eq)]
struct Request {
    id: u16,
    op: u8,
}

#[bin(big)]
#[derive(Debug, PartialEq, Eq)]
struct Reply {
    id: u16,
    status: u8,
}

/// The unit under test — read one request, reply to its sender. Generic over the socket, so it
/// runs unchanged over a real `UdpSocket` (production) or a `MockDatagramSocket` (tests).
fn serve_one<D: DatagramSocket<Addr = SocketAddr>>(
    server: &mut MessageDatagram<D>,
) -> Result<(), bnb::BitError> {
    let (req, from): (Request, _) = server.recv_message()?;
    server.send_message(
        &Reply {
            id: req.id,
            status: 0,
        },
        &from,
    )?;
    Ok(())
}

fn main() {
    // A server over the mock — nothing bound to the network.
    let mut server = MessageDatagram::new(MockDatagramSocket::new());

    // Inject a request as if a client had sent it.
    let client: SocketAddr = "127.0.0.1:40000".parse().unwrap();
    server
        .get_ref()
        .push_inbound(&Request { id: 42, op: 1 }.to_bytes().unwrap(), client);

    // Run the handler, then assert on what it sent.
    serve_one(&mut server).unwrap();

    let sent = server.get_ref().sent();
    assert_eq!(sent.len(), 1);
    let (bytes, dest) = &sent[0];
    assert_eq!(*dest, client); // replied to the sender
    let reply = Reply::decode_exact(bytes).unwrap();
    assert_eq!(reply, Reply { id: 42, status: 0 }); // matching id

    println!("served one request over a mock socket (no UdpSocket bound):");
    println!("  in : Request {{ id: 42, op: 1 }} from {client}");
    println!("  out: {reply:?} -> {dest}");

    // --- error path: the recv fails ---
    // `fail_next_recv` makes the next `recv_from` error — `recv_message` surfaces the I/O error.
    let mut down = MessageDatagram::new(MockDatagramSocket::new().fail_next_recv());
    let err = down.recv_message::<Request>().unwrap_err();
    assert!(matches!(err.kind, bnb::ErrorKind::Io(_)));
    println!("  error path: a failed recv surfaces as {:?}", err.kind);

    println!("all checks passed");
}
