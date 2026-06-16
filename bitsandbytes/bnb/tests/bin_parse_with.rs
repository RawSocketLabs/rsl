//! `parse_with`/`write_with` (ROADMAP Phase 2): the field-level custom-codec escape
//! hatch. `#[br(parse_with = f)]` reads the field with `f(r) -> Result<T, BitError>`
//! and `#[bw(write_with = f)]` writes it with `f(&self.field, w) -> Result<(), _>`.

use bnb::{BitError, Sink, Source, bin, u4};

// A length-prefixed byte run: a u8 count, then that many bytes (read at whatever
// bit offset the cursor is at — the point of the escape hatch).
fn parse_lp<S: Source>(r: &mut S) -> Result<Vec<u8>, BitError> {
    let n: u8 = r.read()?;
    let mut v = Vec::new();
    for _ in 0..n {
        v.push(r.read::<u8>()?);
    }
    Ok(v)
}

fn write_lp<K: Sink>(v: &[u8], w: &mut K) -> Result<(), BitError> {
    w.write(v.len() as u8)?;
    for b in v {
        w.write(*b)?;
    }
    Ok(())
}

#[bin]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Frame {
    tag: u4,
    #[br(parse_with = parse_lp)]
    #[bw(write_with = write_lp)]
    data: Vec<u8>,
}

#[test]
fn custom_codec_round_trips() {
    let f = Frame {
        tag: u4::new(0x5),
        data: vec![0xAA, 0xBB, 0xCC],
    };
    let bytes = f.to_bytes().unwrap();
    assert_eq!(Frame::decode_exact(&bytes).unwrap(), f);
}

#[test]
fn empty_custom_run() {
    let f = Frame {
        tag: u4::new(0),
        data: vec![],
    };
    assert_eq!(Frame::decode_exact(&f.to_bytes().unwrap()).unwrap(), f);
}
