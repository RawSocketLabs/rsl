//! Lists whose length prefix counts bytes, not items (the TLS `T items<a..b>` vector).
//!
//! bnb's `count_prefix` counts elements, so it only fits vectors of single bytes. These
//! functions are the `parse_with`/`write_with` pair for every other vector.
//!
//! The two "until the input ends" readers ([`read_trailing_list16`], [`read_to_end`]) detect
//! the end by an `UnexpectedEof` that reports zero bits remaining. That is only "the message
//! ended" when the source is a slice holding exactly one message body, which is how every
//! body in this crate is decoded (`decode_exact`). A streaming source reports `Incomplete`
//! there, so they fail safe; bnb's `SeekReader` always reports zero remaining and a
//! `limit_bytes` window wider than its data hides unread bytes, so neither may be used.

// --- Workspace dependencies ---
use bnb::{BitDecode, BitEncode, BitError, BitWriter, ErrorKind, Sink, Source, u24};

/// Decodes items until a region of exactly `len` bytes is used up.
fn read_region<T: BitDecode, S: Source>(r: &mut S, len: usize) -> Result<Vec<T>, BitError> {
    let mut region = r.limit_bytes(len)?;
    let mut items = Vec::new();
    while region.remaining_bits() > 0 {
        let before = region.bit_pos();
        items.push(T::bit_decode(&mut region)?);
        // An item that decodes from no bits would fill memory from an empty region.
        if region.bit_pos() == before {
            return Err(BitError::new(ErrorKind::NoProgress, before));
        }
    }
    region.finish()?;
    Ok(items)
}

/// Encodes items back to back, so their total byte length can be written first.
fn encode_items<T: BitEncode>(items: &[T]) -> Result<Vec<u8>, BitError> {
    let mut body = BitWriter::new();
    for item in items {
        item.bit_encode(&mut body)?;
    }
    Ok(body.into_bytes())
}

/// The error for a list whose encoding does not fit its length prefix.
fn too_long<K: Sink>(w: &K) -> BitError {
    BitError::convert(
        "list is longer than its length prefix can express",
        w.bit_pos(),
    )
}

/// Reads a list behind a 16-bit byte length.
pub(crate) fn read_list16<T: BitDecode, S: Source>(r: &mut S) -> Result<Vec<T>, BitError> {
    let len = usize::from(r.read::<u16>()?);
    read_region(r, len)
}

/// Writes a list behind a 16-bit byte length.
pub(crate) fn write_list16<T: BitEncode, K: Sink>(items: &[T], w: &mut K) -> Result<(), BitError> {
    let body = encode_items(items)?;
    let len = u16::try_from(body.len()).map_err(|_| too_long(w))?;
    w.write(len)?;
    w.write_bytes(&body)
}

/// Reads a list behind a 24-bit byte length.
pub(crate) fn read_list24<T: BitDecode, S: Source>(r: &mut S) -> Result<Vec<T>, BitError> {
    let len = r.read::<u24>()?.value() as usize;
    read_region(r, len)
}

/// Writes a list behind a 24-bit byte length.
pub(crate) fn write_list24<T: BitEncode, K: Sink>(items: &[T], w: &mut K) -> Result<(), BitError> {
    let body = encode_items(items)?;
    let len = u24::try_from(body.len()).map_err(|_| too_long(w))?;
    w.write(len)?;
    w.write_bytes(&body)
}

/// Whether a failed read found the input already used up, as opposed to cut short.
fn at_end(error: &BitError) -> bool {
    matches!(error.kind, ErrorKind::UnexpectedEof { remaining: 0, .. })
}

/// Reads the optional list that ends a hello message: absent when no bytes remain
/// (RFC 5246 §7.4.1.2), otherwise a list behind a 16-bit byte length. See the module
/// documentation for the sources this is sound on.
pub(crate) fn read_trailing_list16<T: BitDecode, S: Source>(
    r: &mut S,
) -> Result<Option<Vec<T>>, BitError> {
    match r.read::<u16>() {
        Ok(len) => read_region(r, usize::from(len)).map(Some),
        Err(error) if at_end(&error) => Ok(None),
        Err(error) => Err(error),
    }
}

/// Reads every remaining byte of a message body. See the module documentation for the
/// sources this is sound on.
pub(crate) fn read_to_end<S: Source>(r: &mut S) -> Result<Vec<u8>, BitError> {
    let mut bytes = Vec::new();
    loop {
        match r.read::<u8>() {
            Ok(byte) => bytes.push(byte),
            Err(error) if at_end(&error) => return Ok(bytes),
            Err(error) => return Err(error),
        }
    }
}

/// Writes bytes with no length in front, the counterpart of [`read_to_end`].
pub(crate) fn write_bytes<K: Sink>(bytes: &[u8], w: &mut K) -> Result<(), BitError> {
    w.write_bytes(bytes)
}

/// Writes the optional list that ends a hello message: nothing at all when absent.
#[expect(
    clippy::ref_option,
    reason = "bnb's write_with passes a reference to the field as declared"
)]
pub(crate) fn write_trailing_list16<T: BitEncode, K: Sink>(
    items: &Option<Vec<T>>,
    w: &mut K,
) -> Result<(), BitError> {
    match items {
        Some(items) => write_list16(items, w),
        None => Ok(()),
    }
}

#[cfg(test)]
mod unit {
    use bnb::{BitReader, bin};

    use super::*;

    /// An item that decodes from no bits.
    #[bin(big)]
    #[derive(Debug, PartialEq)]
    struct Empty {}

    #[test]
    fn item_that_consumes_nothing_is_an_error_not_an_endless_list() {
        let mut reader = BitReader::new(&[0, 2, 0xaa, 0xbb]);
        let error = read_list16::<Empty, _>(&mut reader).unwrap_err();
        assert!(matches!(error.kind, ErrorKind::NoProgress), "{error}");
    }

    #[test]
    fn list_is_read_up_to_its_byte_length_and_no_further() {
        let mut reader = BitReader::new(&[0, 4, 0, 1, 0, 2, 0xff]);
        assert_eq!(read_list16::<u16, _>(&mut reader).unwrap(), [1, 2]);
        assert_eq!(reader.read::<u8>().unwrap(), 0xff);
    }

    #[test]
    fn uint24_length_is_three_big_endian_bytes() {
        let mut written = BitWriter::new();
        write_list24(&[0x0102_u16, 0x0304], &mut written).unwrap();
        let wire = written.into_bytes();
        assert_eq!(wire, [0, 0, 4, 1, 2, 3, 4]);
        let mut reader = BitReader::new(&wire);
        assert_eq!(
            read_list24::<u16, _>(&mut reader).unwrap(),
            [0x0102, 0x0304]
        );
    }

    #[test]
    fn list_too_long_for_its_prefix_is_refused_not_truncated() {
        let items = vec![0_u16; 0x8000];
        assert!(write_list16(&items, &mut BitWriter::new()).is_err());
    }
}
