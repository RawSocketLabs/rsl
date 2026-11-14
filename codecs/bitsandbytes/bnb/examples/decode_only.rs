//! **`decode_only`** — a program that only ever parses: a monitor, a sniffer, a log reader.
//!
//! `#[bin(read_only)]` generates the decoder alone (no encoder, no builder), so a wire type
//! the program never builds carries nothing it does not use. The plain transport reads
//! (`read_message`, `recv_message`) ask a type's *encoder* for its byte and bit order; a
//! type with no encoder is read with the `_with` forms instead, where the caller states the
//! order. The same forms read a message that needs context from its caller (`ctx(...)`).
//!
//! Run with: `cargo run -p bitsandbytes --example decode_only --features mock`

use bnb::net::MessageReadError;
use bnb::{
    BitOrder, ByteOrder, Layout, MessageDatagram, MessageStream, MockDatagramSocket, MockStream,
    bin,
};
use std::net::SocketAddr;

/// A sensor report as it appears on the wire: little-endian, a counted run of samples.
#[bin(little, read_only)]
#[derive(Debug, PartialEq)]
struct Report {
    sensor: u8,
    #[brw(count_prefix = u8)]
    samples: Vec<u16>,
}

/// A record whose length an outer header carries: it decodes only with that length in hand.
#[bin(big, read_only, ctx(len: u8))]
#[derive(Debug, PartialEq)]
struct Record {
    kind: u8,
    #[br(count = len)]
    body: Vec<u8>,
}

/// The order `Report` declares. Say it once, next to the types it describes.
const REPORTS: Layout = Layout {
    bit: BitOrder::Msb,
    byte: ByteOrder::Little,
};
const RECORDS: Layout = Layout {
    bit: BitOrder::Msb,
    byte: ByteOrder::Big,
};

#[allow(clippy::print_stdout)] // This CLI demo intentionally prints its observable results.
fn main() {
    // --- a stream: two reports back to back, arriving three bytes per read ---
    let mut stream = MessageStream::new(MockStream::with_chunk_size(3));
    stream
        .get_mut()
        .push_inbound(&[7, 2, 0x34, 0x12, 0x78, 0x56, 9, 0]);

    let first: Report = stream.read_message_with(REPORTS, ()).unwrap();
    let second: Report = stream.read_message_with(REPORTS, ()).unwrap();
    assert_eq!(
        first,
        Report {
            sensor: 7,
            samples: vec![0x1234, 0x5678]
        }
    );
    assert_eq!(
        second,
        Report {
            sensor: 9,
            samples: vec![]
        }
    );
    println!("stream  : {first:?}, then {second:?}");

    // The stream ends between messages: that is the transport closing, not a bad message.
    let end = stream
        .read_message_with::<Report, _>(REPORTS, ())
        .unwrap_err();
    assert!(
        matches!(&end, MessageReadError::Io(e) if e.kind() == std::io::ErrorKind::UnexpectedEof)
    );
    println!("stream  : then {end}");

    // --- context: a one-byte length, then a record of that length ---
    let mut framed = MessageStream::new(MockStream::with_chunk_size(2));
    framed.get_mut().push_inbound(&[3, 0xA1, b'a', b'b', b'c']);
    let len: u8 = framed.read_message().unwrap();
    let record: Record = framed
        .read_message_with(RECORDS, RecordCtx { len })
        .unwrap();
    assert_eq!(
        record,
        Record {
            kind: 0xA1,
            body: b"abc".to_vec()
        }
    );
    println!("context : length {len}, then {record:?}");

    // --- a datagram: one report per datagram, with its sender ---
    let mut socket = MessageDatagram::new(MockDatagramSocket::new());
    let sender: SocketAddr = "127.0.0.1:40000".parse().unwrap();
    socket.get_ref().push_inbound(&[4, 1, 0xEF, 0xBE], sender);
    let (report, from): (Report, _) = socket.recv_message_with(REPORTS, ()).unwrap();
    assert_eq!(
        (report.sensor, &report.samples[..], from),
        (4, &[0xBEEF][..], sender)
    );
    println!("datagram: {report:?} from {from}");

    println!("all checks passed");
}
