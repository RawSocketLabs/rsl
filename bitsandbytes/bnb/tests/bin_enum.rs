//! `#[bin]` on an enum — tagged-union dispatch.
//!
//! Two orthogonal concepts: `magic` is a wire constant (byte string or byte-aligned int)
//! that is read *and* written — the discriminant under magic dispatch, or a verified
//! signature on a tag-variant; `tag` is a read-only selector from `ctx` that picks the
//! variant and is never on the wire. `#[catch_all]` preserves an unknown discriminant
//! (dual-use); without one a magic enum is a closed set (unknown ⇒ decode error).

mod macro_ {

    use bnb::bin;

    #[bin(big)]
    #[derive(Debug, PartialEq)]
    struct Inner {
        x: u16,
    }

    // ── Integer magic dispatch: read a u16, match, dispatch. Every variant shape. ──
    #[bin(big)]
    #[derive(Debug, PartialEq)]
    enum Rdata {
        #[bin(magic = 1u16)]
        A(u32), // tuple newtype
        #[bin(magic = 2u16)]
        Port { lo: u8, hi: u8 }, // struct variant
        #[bin(magic = 3u16)]
        Nested(#[nested] Inner), // a nested #[bin] message
        #[bin(magic = 0u16)]
        Ping, // unit variant: magic only
        #[catch_all]
        Other {
            magic: u16, // first field captures the unmatched discriminant
            #[br(count = 2)]
            raw: Vec<u8>,
        },
    }

    #[test]
    fn int_magic_roundtrips_every_variant_shape() {
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
    fn catch_all_captures_an_unknown_magic_and_roundtrips() {
        let bytes = [0x00, 0x09, 0xAA, 0xBB]; // magic 9 matches nothing
        let decoded = Rdata::decode_exact(&bytes).unwrap();
        assert_eq!(
            decoded,
            Rdata::Other {
                magic: 9,
                raw: vec![0xAA, 0xBB],
            }
        );
        assert_eq!(decoded.to_bytes().unwrap(), bytes); // unknown magic preserved
    }

    #[test]
    fn magic_accessor_reports_each_variants_signature() {
        assert_eq!(Rdata::A(5).magic(), 1);
        assert_eq!(Rdata::Ping.magic(), 0);
        assert_eq!(
            Rdata::Other {
                magic: 99,
                raw: vec![]
            }
            .magic(),
            99
        );
    }

    // ── Byte-string magic dispatch (PNG/RIFF-style signatures). ──
    #[bin(big)]
    #[derive(Debug, PartialEq)]
    enum Chunk {
        #[bin(magic = b"IHDR")]
        Header { width: u16, height: u16 },
        #[bin(magic = b"IDAT")]
        Data(u8),
        #[catch_all]
        Other {
            magic: [u8; 4],
            #[br(count = 1)]
            rest: Vec<u8>,
        },
    }

    #[test]
    fn byte_string_magic_dispatch() {
        let hdr = [b'I', b'H', b'D', b'R', 0x00, 0x10, 0x00, 0x20];
        assert_eq!(
            Chunk::decode_exact(&hdr).unwrap(),
            Chunk::Header {
                width: 16,
                height: 32
            }
        );
        assert_eq!(
            Chunk::Header {
                width: 16,
                height: 32
            }
            .to_bytes()
            .unwrap(),
            hdr
        );
        assert_eq!(
            Chunk::Header {
                width: 0,
                height: 0
            }
            .magic(),
            *b"IHDR"
        );

        let unknown = [b'X', b'X', b'X', b'X', 0x42];
        assert_eq!(
            Chunk::decode_exact(&unknown).unwrap(),
            Chunk::Other {
                magic: *b"XXXX",
                rest: vec![0x42],
            }
        );
        assert_eq!(
            Chunk::decode_exact(&unknown).unwrap().to_bytes().unwrap(),
            unknown
        );
    }

    // ── A closed magic set (no catch-all): an unknown magic is a decode error. ──
    #[bin(big)]
    #[derive(Debug, PartialEq)]
    enum Closed {
        #[bin(magic = 1u8)]
        One(u8),
        #[bin(magic = 2u8)]
        Two(u8),
    }

    #[test]
    fn closed_magic_set_errors_on_unknown() {
        assert_eq!(Closed::decode_exact(&[1, 0x42]).unwrap(), Closed::One(0x42));
        assert_eq!(Closed::decode_exact(&[2, 0x99]).unwrap(), Closed::Two(0x99));
        assert!(Closed::decode_exact(&[9, 0x00]).is_err());
    }

    // ── A leading magic prefix (verified + written once) before dispatch. ──
    #[bin(big, magic = b"BNB")]
    #[derive(Debug, PartialEq)]
    enum Pre {
        #[bin(magic = 1u8)]
        A(u16),
        #[bin(magic = 2u8)]
        B,
    }

    #[test]
    fn enum_level_magic_prefix() {
        let a = [b'B', b'N', b'B', 0x01, 0xCA, 0xFE];
        assert_eq!(Pre::decode_exact(&a).unwrap(), Pre::A(0xCAFE));
        assert_eq!(Pre::A(0xCAFE).to_bytes().unwrap(), a);
        // A wrong prefix is rejected before dispatch.
        assert!(Pre::decode_exact(&[b'X', b'N', b'B', 0x01, 0x00, 0x00]).is_err());
    }

    // ── Tag dispatch: a ctx selector picks the variant; nothing on the wire is the tag. ──
    #[bin(big, ctx(kind: u16), tag = kind)]
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
    fn tag_dispatch_writes_no_discriminant() {
        let p = Packet {
            kind: 1,
            body: Body::Login(0xAABB_CCDD),
        };
        let bytes = [0x00, 0x01, 0xAA, 0xBB, 0xCC, 0xDD]; // kind, then payload only
        assert_eq!(p.to_bytes().unwrap(), bytes);
        assert_eq!(Packet::decode_exact(&bytes).unwrap(), p);

        // Standalone: the enum reads no tag, only the payload (one byte here).
        let b = Body::decode_with_exact(&[0x07], BodyCtx { kind: 2 }).unwrap();
        assert_eq!(b, Body::Data { n: 7 });
        assert_eq!(b.tag(), 2);
    }

    // The no-drift pattern: the tag isn't stored — it's a temp recomputed from `body.tag()`.
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
    fn temp_tag_recomputed_on_encode_and_passed_as_ctx() {
        let p = Packet2 {
            body: Body::Data { n: 7 },
        };
        assert_eq!(p.to_bytes().unwrap(), [0x00, 0x02, 0x07]);
        assert_eq!(Packet2::decode_exact(&[0x00, 0x02, 0x07]).unwrap(), p);
    }

    // ── `decode_as_<variant>` parses the bytes as one explicit variant (bypassing
    //    dispatch); `decode_tagged` feeds a tag enum its selector. ──
    #[test]
    fn decode_as_explicit_variant_magic() {
        assert_eq!(
            Rdata::decode_as_a(&[0x00, 0x01, 0xDE, 0xAD, 0xBE, 0xEF]).unwrap(),
            Rdata::A(0xDEAD_BEEF)
        );
        assert_eq!(
            Rdata::decode_as_port(&[0x00, 0x02, 0x11, 0x22]).unwrap(),
            Rdata::Port { lo: 0x11, hi: 0x22 }
        );
        // A wrong magic for the targeted variant is rejected.
        assert!(Rdata::decode_as_a(&[0x00, 0x09, 0, 0, 0, 0]).is_err());
        // The prefix is verified too.
        assert_eq!(
            Pre::decode_as_a(&[b'B', b'N', b'B', 0x01, 0xCA, 0xFE]).unwrap(),
            Pre::A(0xCAFE)
        );
        assert!(Pre::decode_as_a(&[b'X', b'N', b'B', 0x01, 0x00, 0x00]).is_err());
    }

    #[test]
    fn peek_variant_identifies_by_magic() {
        assert_eq!(
            Rdata::peek_variant(&[0x00, 0x01, 0, 0, 0, 0]).unwrap(),
            RdataKind::A
        );
        assert_eq!(Rdata::peek_variant(&[0x00, 0x00]).unwrap(), RdataKind::Ping);
        assert_eq!(
            Rdata::peek_variant(&[0x00, 0x09]).unwrap(),
            RdataKind::Other
        ); // -> catch-all
        assert_eq!(Chunk::peek_variant(b"IDAT\x00").unwrap(), ChunkKind::Data);
        // A closed set errors on an unknown magic.
        assert!(Closed::peek_variant(&[9, 0]).is_err());
        assert_eq!(Closed::peek_variant(&[1, 0x42]).unwrap(), ClosedKind::One);
    }

    #[test]
    fn decode_as_and_tagged_for_tag_enum() {
        let li = [0xAA, 0xBB, 0xCC, 0xDD];
        // decode_as takes the ctx (Login here ignores it); decode_tagged takes the selector.
        assert_eq!(
            Body::decode_as_login(&li, BodyCtx { kind: 0 }).unwrap(),
            Body::Login(0xAABB_CCDD)
        );
        assert_eq!(
            Body::decode_tagged(1, &li).unwrap(),
            Body::Login(0xAABB_CCDD)
        );
        assert_eq!(
            Body::decode_tagged(2, &[0x07]).unwrap(),
            Body::Data { n: 7 }
        );
    }

    // ── Hybrid: tag dispatch takes priority; an unmatched selector falls to magic. ──
    #[bin(big, ctx(kind: u8), tag = kind)]
    #[derive(Debug, PartialEq)]
    enum Hybrid {
        #[bin(tag = 1)]
        Known(u16),
        #[bin(magic = b"EXT")]
        Extended { sub: u8 },
        #[catch_all]
        Unknown {
            magic: [u8; 3],
            #[br(count = 1)]
            rest: Vec<u8>,
        },
    }

    #[test]
    fn hybrid_tag_then_magic() {
        // kind=1 selects the tag variant — no wire discriminant.
        let known = Hybrid::decode_with_exact(&[0xAB, 0xCD], HybridCtx { kind: 1 }).unwrap();
        assert_eq!(known, Hybrid::Known(0xABCD));
        assert_eq!(known.to_bytes().unwrap(), [0xAB, 0xCD]);

        // An unmatched selector (kind=9) falls through to magic dispatch.
        let ext = Hybrid::decode_with_exact(b"EXT\x05", HybridCtx { kind: 9 }).unwrap();
        assert_eq!(ext, Hybrid::Extended { sub: 5 });
        assert_eq!(ext.to_bytes().unwrap(), b"EXT\x05");

        // Unknown magic -> the catch-all captures it.
        let unk = Hybrid::decode_with_exact(b"ZZZ\xFF", HybridCtx { kind: 9 }).unwrap();
        assert_eq!(
            unk,
            Hybrid::Unknown {
                magic: *b"ZZZ",
                rest: vec![0xFF],
            }
        );
        assert_eq!(unk.to_bytes().unwrap(), b"ZZZ\xFF");
    }

    // ── Variable-width magic: byte-string signatures of differing lengths, peeked and
    //    matched by prefix; the catch-all reads from the unconsumed position. ──
    #[bin(big)]
    #[derive(Debug, PartialEq)]
    enum Frame {
        #[bin(magic = b"LOGIN")]
        Login { user: u32 },
        #[bin(magic = b"BYE")]
        Bye,
        #[catch_all]
        Unknown {
            #[br(count = 2)]
            raw: Vec<u8>,
        },
    }

    #[test]
    fn mixed_width_magic_dispatch() {
        let login = [b'L', b'O', b'G', b'I', b'N', 0xAA, 0xBB, 0xCC, 0xDD];
        assert_eq!(
            Frame::decode_exact(&login).unwrap(),
            Frame::Login { user: 0xAABB_CCDD }
        );
        assert_eq!(
            Frame::Login { user: 0xAABB_CCDD }.to_bytes().unwrap(),
            login
        );
        assert_eq!(Frame::decode_exact(b"BYE").unwrap(), Frame::Bye);
        assert_eq!(Frame::Bye.to_bytes().unwrap(), b"BYE");

        // Unknown: nothing matches, so the catch-all reads from the unconsumed start — the
        // bytes (including what would be a magic) live in its own field, and round-trip.
        let unk = [0xDE, 0xAD];
        assert_eq!(
            Frame::decode_exact(&unk).unwrap(),
            Frame::Unknown {
                raw: vec![0xDE, 0xAD]
            }
        );
        assert_eq!(
            Frame::Unknown {
                raw: vec![0xDE, 0xAD]
            }
            .to_bytes()
            .unwrap(),
            unk
        );
        assert_eq!(Frame::peek_variant(&login).unwrap(), FrameKind::Login);
        assert_eq!(Frame::peek_variant(&unk).unwrap(), FrameKind::Unknown);
    }

    // ── A typed fallback: magic variants, then a no-magic/no-tag variant parsed when none
    //    matches (parsing from the unconsumed position). ──
    #[bin(big)]
    #[derive(Debug, PartialEq)]
    enum Wire {
        #[bin(magic = b"PING")]
        Ping,
        #[bin(magic = b"PONG")]
        Pong,
        Raw {
            len: u8,
            #[br(count = len)]
            body: Vec<u8>,
        },
    }

    #[test]
    fn typed_fallback_when_no_magic_matches() {
        assert_eq!(Wire::decode_exact(b"PING").unwrap(), Wire::Ping);
        assert_eq!(Wire::Pong.to_bytes().unwrap(), b"PONG");
        // Not PING/PONG -> the typed fallback parses from the start.
        let raw = [0x02, 0xAA, 0xBB];
        assert_eq!(
            Wire::decode_exact(&raw).unwrap(),
            Wire::Raw {
                len: 2,
                body: vec![0xAA, 0xBB]
            }
        );
        assert_eq!(
            Wire::Raw {
                len: 2,
                body: vec![0xAA, 0xBB]
            }
            .to_bytes()
            .unwrap(),
            raw
        );
        assert_eq!(Wire::peek_variant(b"PONG").unwrap(), WireKind::Pong);
        assert_eq!(Wire::peek_variant(&raw).unwrap(), WireKind::Raw);
    }

    // ── `tag` + `magic` compose: the selector picks the variant, then its signature is
    //    verified on read and written on encode (it IS on the wire; the tag is not). ──
    #[bin(big, ctx(kind: u8), tag = kind)]
    #[derive(Debug, PartialEq)]
    enum Msg {
        #[bin(tag = 1, magic = b"LI")]
        Login(u32),
        #[bin(tag = 2)]
        Ping, // no signature
    }

    #[test]
    fn tag_with_verification_magic() {
        // kind=1 selects Login; "LI" is then verified and the u32 read.
        let li = [b'L', b'I', 0xAA, 0xBB, 0xCC, 0xDD];
        let v = Msg::decode_with_exact(&li, MsgCtx { kind: 1 }).unwrap();
        assert_eq!(v, Msg::Login(0xAABB_CCDD));
        assert_eq!(v.to_bytes().unwrap(), li);

        // A bad signature for the selected variant is rejected.
        assert!(Msg::decode_with_exact(&[b'X', b'X', 0, 0, 0, 0], MsgCtx { kind: 1 }).is_err());

        // kind=2 selects Ping (no signature, no payload).
        assert_eq!(
            Msg::decode_with_exact(&[], MsgCtx { kind: 2 }).unwrap(),
            Msg::Ping
        );
        assert_eq!(Msg::Ping.to_bytes().unwrap(), Vec::<u8>::new());
    }

    // ── Variant-field directives still work under the new dispatch. ──

    // `temp` + `calc` on a catch-all field: read its own length, recompute on encode.
    #[bin(big)]
    #[derive(Debug, PartialEq)]
    enum Tlv {
        #[bin(magic = 1u8)]
        Ping,
        #[catch_all]
        Unknown {
            magic: u8,
            #[br(temp)]
            #[bw(calc = body.len() as u8)]
            len: u8,
            #[br(count = len)]
            body: Vec<u8>,
        },
    }

    #[test]
    fn temp_calc_length_on_a_catch_all_field() {
        let bytes = [0x09, 0x03, 0xAA, 0xBB, 0xCC]; // magic 9, len 3, 3 payload bytes
        let v = Tlv::decode_exact(&bytes).unwrap();
        assert_eq!(
            v,
            Tlv::Unknown {
                magic: 9,
                body: vec![0xAA, 0xBB, 0xCC],
            }
        );
        assert_eq!(v.to_bytes().unwrap(), bytes);
    }

    // `ctx` on a variant field: the payload is itself a ctx-message taking a sibling length.
    #[bin(big, ctx(n: u8))]
    #[derive(Debug, PartialEq)]
    struct SizedBlob {
        #[br(count = n)]
        bytes: Vec<u8>,
    }

    #[bin(big)]
    #[derive(Debug, PartialEq)]
    enum Framed {
        #[bin(magic = 1u8)]
        Blob {
            n: u8,
            #[br(ctx { n })]
            data: SizedBlob,
        },
    }

    #[test]
    fn ctx_on_a_variant_field() {
        let bytes = [0x01, 0x02, 0xAA, 0xBB]; // magic 1, n=2, two payload bytes
        let f = Framed::decode_exact(&bytes).unwrap();
        assert_eq!(
            f,
            Framed::Blob {
                n: 2,
                data: SizedBlob {
                    bytes: vec![0xAA, 0xBB],
                },
            }
        );
        assert_eq!(f.to_bytes().unwrap(), bytes);
    }
}
