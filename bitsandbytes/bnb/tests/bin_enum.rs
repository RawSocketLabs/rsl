//! `#[bin]` on an enum — tag-dispatched tagged unions.
//!
//! A discriminant (read via `tag = <ty>`, or taken from a `ctx` param via
//! `tag_from = <param>`) selects the variant; `#[catch_all]` preserves an unknown tag
//! and its payload (dual-use). Variants may be unit, tuple, named, or `#[nested]`.

use bnb::bin;

#[bin(big)]
#[derive(Debug, PartialEq)]
struct Inner {
    x: u16,
}

// Internal tag: read a u16, then dispatch. Mixed variant shapes + a catch-all.
#[bin(big, tag = u16)]
#[derive(Debug, PartialEq)]
enum Rdata {
    #[bin(tag = 1)]
    A(u32), // tuple newtype
    #[bin(tag = 2)]
    Port { lo: u8, hi: u8 }, // struct variant
    #[bin(tag = 3)]
    Nested(#[nested] Inner), // a nested #[bin] message
    #[bin(tag = 0)]
    Ping, // unit variant: tag only
    #[catch_all]
    Other {
        tag: u16, // first field captures the unmatched discriminant
        #[br(count = 2)]
        raw: Vec<u8>,
    },
}

#[test]
fn internal_tag_roundtrips_every_variant_shape() {
    let cases: &[(Rdata, &[u8])] = &[
        (Rdata::A(0xDEAD_BEEF), &[0x00, 0x01, 0xDE, 0xAD, 0xBE, 0xEF]),
        (
            Rdata::Port { lo: 0x11, hi: 0x22 },
            &[0x00, 0x02, 0x11, 0x22],
        ),
        (
            Rdata::Nested(Inner { x: 0xBEEF }),
            &[0x00, 0x03, 0xBE, 0xEF],
        ),
        (Rdata::Ping, &[0x00, 0x00]),
    ];
    for (val, bytes) in cases {
        assert_eq!(&val.to_bytes().unwrap(), bytes, "encode {val:?}");
        assert_eq!(
            &Rdata::decode_exact(bytes).unwrap(),
            val,
            "decode {bytes:?}"
        );
    }
}

#[test]
fn catch_all_preserves_an_unknown_tag_and_roundtrips() {
    // Tag 9 matches no variant -> Other captures it plus the 2 raw payload bytes.
    let bytes = [0x00, 0x09, 0xAA, 0xBB];
    let decoded = Rdata::decode_exact(&bytes).unwrap();
    assert_eq!(
        decoded,
        Rdata::Other {
            tag: 9,
            raw: vec![0xAA, 0xBB],
        }
    );
    // ...and it round-trips: the unknown tag goes back on the wire unchanged.
    assert_eq!(decoded.to_bytes().unwrap(), bytes);
}

#[test]
fn tag_accessor_reports_each_variants_discriminant() {
    assert_eq!(Rdata::A(5).tag(), 1);
    assert_eq!(Rdata::Port { lo: 0, hi: 0 }.tag(), 2);
    assert_eq!(Rdata::Ping.tag(), 0);
    assert_eq!(
        Rdata::Other {
            tag: 99,
            raw: vec![]
        }
        .tag(),
        99
    );
}

// A closed union (no `#[catch_all]`): an unrecognized tag is a decode error.
#[bin(big, tag = u8)]
#[derive(Debug, PartialEq)]
enum Closed {
    #[bin(tag = 1)]
    One(u8),
    #[bin(tag = 2)]
    Two(u8),
}

#[test]
fn closed_union_errors_on_unknown_tag() {
    assert_eq!(Closed::decode_exact(&[1, 0x42]).unwrap(), Closed::One(0x42));
    assert_eq!(Closed::decode_exact(&[2, 0x99]).unwrap(), Closed::Two(0x99));
    assert!(Closed::decode_exact(&[9, 0x00]).is_err()); // no catch_all -> rejected
}

// External tag: the parent reads `kind` and hands it down; the enum reads no tag.
#[bin(big, ctx(kind: u16), tag_from = kind)]
#[derive(Debug, PartialEq)]
enum Body {
    #[bin(tag = 1)]
    Login(u32),
    #[bin(tag = 2)]
    Data { n: u8 },
}

#[bin(big)]
#[derive(Debug, PartialEq)]
struct Packet {
    kind: u16,
    #[br(ctx { kind })]
    body: Body,
}

#[test]
fn external_tag_dispatches_on_parent_context() {
    let p = Packet {
        kind: 1,
        body: Body::Login(0xAABB_CCDD),
    };
    let bytes = [0x00, 0x01, 0xAA, 0xBB, 0xCC, 0xDD];
    assert_eq!(p.to_bytes().unwrap(), bytes);
    assert_eq!(Packet::decode_exact(&bytes).unwrap(), p);

    // The enum can also be driven standalone via its generated `*_with` API.
    let b = Body::decode_with_exact(&[0x07], BodyCtx { kind: 2 }).unwrap();
    assert_eq!(b, Body::Data { n: 7 });
}

#[test]
fn tag_accessor_drives_a_parents_no_drift_tag() {
    // `tag()` lets the parent recompute the discriminant from the chosen variant
    // (`#[bw(calc = self.body.tag())]`) when the tag is stored as a normal field.
    let p = Packet {
        kind: Body::Data { n: 7 }.tag(),
        body: Body::Data { n: 7 },
    };
    assert_eq!(p.kind, 2);
    assert_eq!(p.to_bytes().unwrap(), [0x00, 0x02, 0x07]);
}

// The no-drift pattern in full: the tag isn't stored at all — it is read into a temp on
// decode and recomputed from the chosen variant on encode, then handed to the union.
#[bin(big)]
#[derive(Debug, PartialEq)]
struct Packet2 {
    #[br(temp)]
    #[bw(calc = self.body.tag())]
    kind: u16,
    #[br(ctx { kind })]
    body: Body,
}

#[test]
fn temp_tag_is_recomputed_on_encode_and_passed_as_ctx() {
    let p = Packet2 {
        body: Body::Data { n: 7 },
    };
    // `kind` is not a field of Packet2; on encode it is `body.tag()` == 2, written then
    // passed to `body` as ctx; on decode it is a temp read and passed down.
    assert_eq!(p.to_bytes().unwrap(), [0x00, 0x02, 0x07]);
    assert_eq!(Packet2::decode_exact(&[0x00, 0x02, 0x07]).unwrap(), p);

    let q = Packet2 {
        body: Body::Login(0x0102_0304),
    };
    assert_eq!(q.to_bytes().unwrap(), [0x00, 0x01, 0x01, 0x02, 0x03, 0x04]);
    assert_eq!(
        Packet2::decode_exact(&[0x00, 0x01, 0x01, 0x02, 0x03, 0x04]).unwrap(),
        q
    );
}

// `temp` + `calc` on a VARIANT field: a length-prefixed catch-all that reads its own
// length on decode and recomputes it from the payload on encode (never stored, never
// drifts) — the killer feature for self-delimiting unknown records.
#[bin(big, tag = u8)]
#[derive(Debug, PartialEq)]
enum Tlv {
    #[bin(tag = 1)]
    Ping,
    #[catch_all]
    Unknown {
        tag: u8,
        #[br(temp)]
        #[bw(calc = body.len() as u8)] // sees the sibling `body: &Vec<u8>` on encode
        len: u8,
        #[br(count = len)]
        body: Vec<u8>,
    },
}

#[test]
fn temp_calc_length_on_a_variant_field() {
    // tag 9 is unknown -> Unknown captures it, reads len=3, then 3 payload bytes.
    let bytes = [0x09, 0x03, 0xAA, 0xBB, 0xCC];
    let v = Tlv::decode_exact(&bytes).unwrap();
    assert_eq!(
        v,
        Tlv::Unknown {
            tag: 9,
            body: vec![0xAA, 0xBB, 0xCC], // `len` is not a field — it was temp
        }
    );
    // On encode `len` is recomputed from body.len() == 3, so it round-trips.
    assert_eq!(v.to_bytes().unwrap(), bytes);
}

// `ctx` on a VARIANT field: the payload is itself a ctx-message taking a length from an
// earlier sibling of the same variant.
#[bin(big, ctx(n: u8))]
#[derive(Debug, PartialEq)]
struct Sized {
    #[br(count = n)]
    bytes: Vec<u8>,
}

#[bin(big, tag = u8)]
#[derive(Debug, PartialEq)]
enum Framed {
    #[bin(tag = 1)]
    Blob {
        n: u8,
        #[br(ctx { n })] // hand the sibling `n` to `Sized`'s ctx param
        data: Sized,
    },
}

#[test]
fn ctx_on_a_variant_field() {
    let bytes = [0x01, 0x02, 0xAA, 0xBB];
    let f = Framed::decode_exact(&bytes).unwrap();
    assert_eq!(
        f,
        Framed::Blob {
            n: 2,
            data: Sized {
                bytes: vec![0xAA, 0xBB],
            },
        }
    );
    assert_eq!(f.to_bytes().unwrap(), bytes);
}
