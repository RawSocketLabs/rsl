//! **`regions`** — a container of length-delimited records, each decoded inside its own
//! region.
//!
//! A record says how many bytes its body takes. Decoding the body straight from the
//! container's reader trusts the body's decoder to stop where the length says: a body
//! with a count of its own can read into the next record, and a newer sender's longer
//! body leaves bytes behind that are then taken for the next record.
//! [`Source::limit_bytes`] lends the reader through a window of exactly that many bytes:
//! reads past its end fail *inside the record*, [`LimitedSource::finish`] rejects a body
//! that stopped short, and a body this program does not understand is skipped
//! ([`Source::skip_bytes`]). All of it works on a reader that cannot seek.
//!
//! Run with: `cargo run -p bitsandbytes --example regions`
//!
//! [`Source::limit_bytes`]: bnb::Source::limit_bytes
//! [`LimitedSource::finish`]: bnb::LimitedSource::finish
//! [`Source::skip_bytes`]: bnb::Source::skip_bytes

use bnb::{BitDecode, BitError, ErrorKind, Source, StreamBitReader, bin};

/// A body with a count of its own: the kind of decoder a forged count sends past its end.
#[bin(big, read_only)]
#[derive(Debug, PartialEq)]
struct Route {
    metric: u8,
    #[brw(count_prefix = u8)]
    hops: Vec<u16>,
}

#[derive(Debug, PartialEq)]
enum Body {
    Route(Route),
    Name(Vec<u8>),
    /// A kind this version does not know, kept as its number: the body was stepped over.
    Unknown(u8),
}

/// One record: a kind, a one-byte body length, and the body inside a region of that length.
fn record<S: Source>(r: &mut S) -> Result<Body, BitError> {
    let kind: u8 = r.read()?;
    let len = usize::from(r.read::<u8>()?);
    let mut region = r.limit_bytes(len)?;
    let body = match kind {
        1 => Body::Route(Route::bit_decode(&mut region)?),
        2 => Body::Name(region.read_bytes(len)?),
        _ => {
            // Not ours to parse: skip it. A skip, not a seek: it works on a forward-only
            // reader, and it fails if the bytes the length promised never arrive.
            region.skip_bytes(len)?;
            Body::Unknown(kind)
        }
    };
    // The body must use its region exactly: bytes left over mean we misread it.
    region.finish()?;
    Ok(body)
}

#[bin(big, read_only)]
#[derive(Debug, PartialEq)]
struct Record {
    #[br(parse_with = record)]
    body: Body,
}

#[allow(clippy::print_stdout)] // This CLI demo intentionally prints its observable results.
fn main() {
    let wire = [
        1, 6, 9, 2, 0x00, 0x0A, 0x00, 0x0B, // a route: metric 9, two hops
        7, 3, 0xDE, 0xAD, 0xFF, // a kind from a newer version, three bytes
        2, 2, b'h', b'i', // a name
    ];
    let records = Record::decode_all(&wire).unwrap();
    assert_eq!(
        records.iter().map(|r| &r.body).collect::<Vec<_>>(),
        [
            &Body::Route(Route {
                metric: 9,
                hops: vec![10, 11]
            }),
            &Body::Unknown(7),
            &Body::Name(b"hi".to_vec()),
        ]
    );
    println!("decoded : {records:?}");

    // The same from a reader that only goes forward (a socket, a pipe).
    let mut stream = StreamBitReader::new(&wire[..]);
    for expected in &records {
        assert_eq!(&Record::decode(&mut stream).unwrap(), expected);
    }
    println!("stream  : the same three records from a forward-only reader");

    // An unknown kind whose length promises more than there is: an error, not a skip.
    let error = Record::decode_all(&[7, 200, 0xDE]).unwrap_err();
    assert!(matches!(error.kind, ErrorKind::UnexpectedEof { .. }));
    println!("short   : {error}");

    // A route whose own count claims three hops inside a region that holds one. Without
    // the region it would read the name record's bytes as hops; with it, the read fails
    // where the region ends and names how much was there.
    let forged = [1, 4, 9, 3, 0x00, 0x0A, 2, 2, b'h', b'i'];
    let error = Record::decode_all(&forged).unwrap_err();
    assert!(matches!(error.kind, ErrorKind::UnexpectedEof { .. }));
    println!("overrun : {error}");

    // A route that stops short of its region: one hop decoded, two bytes left over.
    let slack = [1, 6, 9, 1, 0x00, 0x0A, 0xEE, 0xEE];
    let error = Record::decode_all(&slack).unwrap_err();
    assert_eq!(error.kind, ErrorKind::TrailingBits { remaining: 16 });
    println!("slack   : {error}");

    println!("all checks passed");
}
