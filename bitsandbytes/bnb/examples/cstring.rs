//! **cstring** — a third `parse_with`/`write_with` shape: a **NUL-terminated** C string (read
//! bytes until `0x00`, write them + a terminator). Distinct from `varint` (LEB128) and `dns`
//! (compression pointers) — the point of the field-level escape hatch is that *you* decide the
//! framing.
//!
//! Run with: `cargo run -p bitsandbytes --example cstring`

use bnb::{BitError, Sink, Source, bin};

/// Read bytes up to (and consuming) a `0x00` terminator.
fn parse_cstr<S: Source>(r: &mut S) -> Result<Vec<u8>, BitError> {
    let mut v = Vec::new();
    loop {
        let b: u8 = r.read()?;
        if b == 0 {
            break;
        }
        v.push(b);
    }
    Ok(v)
}

/// Write the bytes followed by the `0x00` terminator.
fn write_cstr<K: Sink>(v: &[u8], w: &mut K) -> Result<(), BitError> {
    for &b in v {
        w.write(b)?;
    }
    w.write(0u8)?;
    Ok(())
}

#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Entry {
    id: u16,
    #[br(parse_with = parse_cstr)]
    #[bw(write_with = write_cstr)]
    #[try_str]
    name: Vec<u8>,
    flags: u8,
}

fn main() {
    let e = Entry {
        id: 42,
        name: b"alpha".to_vec(),
        flags: 0x01,
    };
    let bytes = e.to_bytes().unwrap();
    // id(2) | "alpha" | 00 (terminator) | flags(1)
    println!("encoded: {bytes:02x?}");
    assert_eq!(bytes[2..8], *b"alpha\0");
    assert_eq!(Entry::decode_exact(&bytes).unwrap(), e);

    // The name is a byte buffer; rendered as text it reads naturally.
    println!("name = {:?}", String::from_utf8_lossy(&e.name));
    println!("all checks passed");
}
