//! `calc`/`temp` (ROADMAP Phase 2, P2.4): `#[br(temp)] #[bw(calc = …)]` reads a
//! field into a local (usable by a later `count`) but does **not** store it —
//! `#[bin]` drops it from the struct and the builder — and recomputes it on write,
//! so the on-wire length can't drift from the `Vec`.
//!
//! `#[br(calc = …)]` is the read-side dual: a **stored** field bound on decode from
//! earlier fields (with no wire read) and written as nothing on encode — its bytes
//! live in the raw fields it derives from. Together with `#[br(temp)] #[bw(calc)]`
//! for the raw layout, it resolves a field whose typed meaning depends on a *later*
//! field without a look-ahead: read the raw bits into temps in wire order, then
//! `calc` the typed field from the full set.

mod macro_ {

    use bnb::{bin, u4};

    #[bin]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct Frame {
        tag: u4,
        #[br(temp)]
        #[bw(calc = self.items.len() as u16)]
        count: u16,
        #[br(count = count)]
        items: Vec<u8>,
    }

    #[test]
    fn temp_field_is_dropped_and_recomputed() {
        // `count` is not a field — the struct literal omits it (compile-time proof),
        // and the on-wire count is recomputed from `items` on every encode.
        for n in [0usize, 1, 3, 7] {
            let f = Frame {
                tag: u4::new(0x5),
                items: vec![0xAB; n],
            };
            let bytes = f.to_bytes().unwrap();
            let decoded = Frame::decode_exact(&bytes).unwrap();
            assert_eq!(decoded, f);
            // Round-trip success across n proves the calc'd count matched items.len():
            // a wrong count would read the wrong number of elements.
            assert_eq!(decoded.items.len(), n);
        }
    }

    #[test]
    fn builder_has_no_temp_field() {
        // The builder is over the cleaned struct, so it has `tag`/`items` but no
        // `count` (temp ⇒ not stored ⇒ not a builder field).
        let f = Frame::builder()
            .tag(u4::new(0xA))
            .items(vec![0x11, 0x22])
            .build()
            .unwrap();
        assert_eq!(Frame::decode_exact(&f.to_bytes().unwrap()).unwrap(), f);
    }

    use bnb::{u2, u3};

    /// A typed value whose two wire bits mean different things depending on the
    /// `outbound` flag — which sits *after* them on the wire. This is the shape
    /// (NXDN's LICH, DMR's directional tables) that a forward-only reader can't
    /// interpret in place.
    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    enum Kind {
        A,
        B,
        Reserved(u2),
    }

    impl Kind {
        fn interpret(bits: u2, outbound: bool) -> Self {
            match (outbound, bits.value()) {
                (true, 0b00) => Kind::A,
                (false, 0b01) => Kind::B,
                _ => Kind::Reserved(bits),
            }
        }

        fn bits(self) -> u2 {
            match self {
                Kind::A => u2::new(0b00),
                Kind::B => u2::new(0b01),
                Kind::Reserved(bits) => bits,
            }
        }
    }

    #[bin]
    #[derive(Debug, PartialEq, Eq, Clone)]
    struct Tagged {
        header: u3,
        // The raw kind bits, read positionally into a local and recomputed from the
        // typed `kind` on write.
        #[br(temp)]
        #[bw(calc = self.kind.bits())]
        raw_kind: u2,
        // The context that gives `raw_kind` its meaning — later on the wire.
        outbound: bool,
        trailing: u2,
        // Resolved after the whole byte is read: depends on `outbound`, which was
        // read after the raw bits. Writes nothing (its bytes are `raw_kind`'s).
        #[br(calc = Kind::interpret(raw_kind, outbound))]
        kind: Kind,
    }

    #[test]
    fn br_calc_resolves_a_field_from_a_later_field_and_round_trips() {
        for value in [
            Tagged {
                header: u3::new(0b101),
                outbound: true,
                trailing: u2::new(0b10),
                kind: Kind::A,
            },
            Tagged {
                header: u3::new(0),
                outbound: false,
                trailing: u2::new(0),
                kind: Kind::B,
            },
            Tagged {
                header: u3::new(0b111),
                outbound: true,
                trailing: u2::new(0b11),
                kind: Kind::Reserved(u2::new(0b11)),
            },
        ] {
            let bytes = value.to_bytes().unwrap();
            assert_eq!(bytes.len(), 1, "the whole record is one byte");
            assert_eq!(Tagged::decode_exact(&bytes).unwrap(), value);
        }
    }

    #[test]
    fn the_later_field_changes_the_interpretation_of_the_same_bits() {
        // Both write raw kind bits `00`; only `outbound` differs. Decoding each
        // reads the same `00` but resolves a different `kind` — proof the later
        // field drives the interpretation, and that both directions round-trip.
        let outbound = Tagged {
            header: u3::new(0b101),
            outbound: true,
            trailing: u2::new(0),
            kind: Kind::A, // A.bits() == 0b00
        };
        let inbound = Tagged {
            header: u3::new(0b101),
            outbound: false,
            trailing: u2::new(0),
            kind: Kind::Reserved(u2::new(0b00)), // also 0b00 on the wire
        };

        let ob = outbound.to_bytes().unwrap();
        let ib = inbound.to_bytes().unwrap();
        // Identical except the single `outbound` bit.
        assert_eq!(ob[0].count_ones().abs_diff(ib[0].count_ones()), 1);

        assert_eq!(Tagged::decode_exact(&ob).unwrap().kind, Kind::A);
        assert_eq!(
            Tagged::decode_exact(&ib).unwrap().kind,
            Kind::Reserved(u2::new(0b00))
        );
    }

    #[test]
    fn br_calc_is_a_normal_stored_builder_field() {
        // Unlike `temp`, a `br(calc)` field is stored — so it is a builder field
        // and appears on the struct. `raw_kind` (temp) is absent.
        let built = Tagged::builder()
            .header(u3::new(0b010))
            .outbound(true)
            .trailing(u2::new(0b01))
            .kind(Kind::A)
            .build()
            .unwrap();
        assert_eq!(
            Tagged::decode_exact(&built.to_bytes().unwrap()).unwrap(),
            built
        );
    }
}
