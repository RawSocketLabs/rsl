//! `Source::as_read` / `Sink::as_write`: hand a bnb cursor to `std::io`-based code
//! inside `parse_with`/`write_with`. The `From<io::Error> for BitError` impl lets such
//! code `?` straight into the codec's error type.

use bnb::{BitError, Sink, Source, bin};
use std::io::{Read, Write};

// A length-prefixed blob, read and written through std::io::Read/Write *views* over the
// bnb cursor — exactly how you'd drop in a `Read`/`Write`-based parser or a stream
// wrapper (decompressor, checksummer, …) from a custom codec.
fn read_blob<S: Source>(r: &mut S) -> Result<Vec<u8>, BitError> {
    let len: u8 = r.read()?;
    let mut buf = vec![0u8; len as usize];
    r.as_read().read_exact(&mut buf)?; // io::Error -> BitError via `?`
    Ok(buf)
}

fn write_blob<K: Sink>(blob: &[u8], w: &mut K) -> Result<(), BitError> {
    w.write(u8::try_from(blob.len()).unwrap())?;
    w.as_write().write_all(blob)?;
    Ok(())
}

#[bin(big)]
#[derive(Debug, PartialEq)]
struct Msg {
    #[br(parse_with = read_blob)]
    #[bw(write_with = write_blob)]
    data: Vec<u8>,
}

#[test]
fn as_read_as_write_roundtrip_through_std_io() {
    let m = Msg {
        data: vec![0xDE, 0xAD, 0xBE, 0xEF],
    };
    let bytes = m.to_bytes().unwrap();
    assert_eq!(bytes, [0x04, 0xDE, 0xAD, 0xBE, 0xEF]);
    assert_eq!(Msg::decode_exact(&bytes).unwrap(), m);
}

#[test]
fn as_read_short_read_reports_eof() {
    use bnb::BitReader;
    // Only 2 bytes available but the length prefix claims 4 -> read_exact hits EOF,
    // which surfaces as a BitError (not a panic).
    let mut r = BitReader::new(&[0x04, 0xAA, 0xBB]);
    assert!(read_blob(&mut r).is_err());
}
