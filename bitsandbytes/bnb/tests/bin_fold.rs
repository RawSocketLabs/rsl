//! Phase 2 capstone — `#[wire]`/`#[bitwire]` folded into `#[bin]`. `#[bin]` is the
//! unified codec: it now handles **byte-aligned** messages natively (no binrw, no
//! right-tool guard), with the full directive surface the old `#[wire]` had — magic,
//! a derived/`temp` count, a count-driven `Vec`, and the builder.

use bnb::bin;

#[bin(magic = 0x7Fu8)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Header {
    version: u8,
    #[br(temp)]
    #[bw(calc = self.records.len() as u16)]
    count: u16,
    #[br(count = count)]
    records: Vec<u32>,
}

#[test]
fn byte_aligned_header_round_trips_via_bin() {
    let h = Header {
        version: 1,
        records: vec![0xAABB_CCDD, 0x1122_3344],
    };
    let bytes = h.to_bytes().unwrap();
    // magic | version | count=2 (BE u16) | 2 × u32 (BE)
    assert_eq!(bytes[0], 0x7F);
    assert_eq!(&bytes[1..4], &[0x01, 0x00, 0x02]);
    assert_eq!(Header::decode_exact(&bytes).unwrap(), h);
}

#[test]
fn builder_excludes_the_temp_count() {
    let h = Header::builder()
        .version(9)
        .records(vec![0x1])
        .build()
        .unwrap();
    assert_eq!(Header::decode_exact(&h.to_bytes().unwrap()).unwrap(), h);
}
