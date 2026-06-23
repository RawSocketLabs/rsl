//! **varint** — `parse_with`/`write_with`: a custom field codec for **LEB128** variable-length
//! integers (the encoding protobuf, DWARF, WASM, and git use). The field-level escape hatch for
//! a codec `bnb` doesn't build in — `parse_with` reads from the cursor, `write_with` writes to
//! it, both at whatever bit offset the surrounding message left it. (A different `parse_with`
//! shape from `dns`'s name compression.)
//!
//! Run with: `cargo run -p bitsandbytes --example varint`

use bnb::{BitError, Sink, Source, bin};

/// Read an unsigned LEB128: 7 payload bits per byte, low group first; the high bit means
/// "another byte follows".
fn parse_varint<S: Source>(r: &mut S) -> Result<u64, BitError> {
    let mut value = 0u64;
    let mut shift = 0u32;
    loop {
        let byte: u8 = r.read()?;
        value |= u64::from(byte & 0x7F) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    Ok(value)
}

/// Write an unsigned LEB128.
fn write_varint<K: Sink>(v: &u64, w: &mut K) -> Result<(), BitError> {
    let mut value = *v;
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80; // more bytes follow
        }
        w.write(byte)?;
        if value == 0 {
            break;
        }
    }
    Ok(())
}

#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Record {
    kind: u8,
    #[br(parse_with = parse_varint)]
    #[bw(write_with = write_varint)]
    length: u64,
    #[br(parse_with = parse_varint)]
    #[bw(write_with = write_varint)]
    timestamp: u64,
}

fn main() {
    // Small values pack into one byte; large ones grow only as needed — the point of LEB128.
    for &(length, timestamp) in &[(0u64, 0u64), (127, 128), (300, 1_000_000), (u64::MAX, 1)] {
        let r = Record {
            kind: 1,
            length,
            timestamp,
        };
        let bytes = r.to_bytes().unwrap();
        println!(
            "len={length} ts={timestamp} -> {} bytes  {bytes:02x?}",
            bytes.len()
        );
        assert_eq!(Record::decode_exact(&bytes).unwrap(), r);
    }
    println!("all checks passed");
}
