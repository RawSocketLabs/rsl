//! **mock_stream** — testing `MessageStream` code with the `mock` feature, no real socket.
//!
//! [`MockStream`](bnb::MockStream) is a `Read + Write` with separate scripted-inbound and
//! captured-outbound buffers — the stream analog of [`MockDatagramSocket`](bnb::MockDatagramSocket).
//! `MessageStream` is generic over `Read + Write` (no sealed trait here), so write your handler
//! generic and test it over the mock; deploy it over a `TcpStream`. The mock can dribble a message
//! **one byte per read** ([`with_chunk_size`]) to exercise `read_message`'s buffer-more-and-retry
//! loop — something `std::io::Cursor` (one read = everything, one shared cursor) can't do.
//!
//! Run with: `cargo run -p bitsandbytes --example mock_stream --features mock`
//!
//! [`with_chunk_size`]: bnb::MockStream::with_chunk_size

use bnb::{MessageStream, MockStream, bin};
use std::io::{Read, Write};

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

/// The unit under test — read one request, write a reply. Generic over the transport, so it runs
/// unchanged over a real `TcpStream` (production) or a `MockStream` (tests).
fn serve_one<S: Read + Write>(conn: &mut MessageStream<S>) -> Result<(), bnb::BitError> {
    let req: Request = conn.read_message()?;
    conn.write_message(&Reply {
        id: req.id,
        status: 0,
    })
}

fn main() {
    // chunk_size = 1: the 3-byte request arrives one byte per read, so `read_message` loops
    // (buffer-more-and-retry) — the framing path a `Cursor` can't simulate.
    let mut conn = MessageStream::new(MockStream::with_chunk_size(1));
    conn.get_mut()
        .push_inbound(&Request { id: 42, op: 1 }.to_bytes().unwrap());

    serve_one(&mut conn).unwrap();

    // assert on the bytes the handler wrote
    let reply = Reply::decode_exact(conn.get_mut().written()).unwrap();
    assert_eq!(reply, Reply { id: 42, status: 0 });

    println!("served one request over a mock stream (1 byte per read, no TcpStream bound):");
    println!("  in : Request {{ id: 42, op: 1 }}");
    println!("  out: {reply:?}");

    // --- error path: the connection drops mid-message ---
    // Deliver only 2 of the request's 3 bytes, then reset — `read_message` surfaces the I/O error.
    let mut dropped = MessageStream::new(MockStream::with_chunk_size(1).fail_after(2));
    dropped
        .get_mut()
        .push_inbound(&Request { id: 9, op: 0 }.to_bytes().unwrap());
    let err = dropped.read_message::<Request>().unwrap_err();
    assert!(matches!(err.kind, bnb::ErrorKind::Io(_)));
    println!(
        "  error path: a mid-message reset surfaces as {:?}",
        err.kind
    );

    println!("all checks passed");
}
