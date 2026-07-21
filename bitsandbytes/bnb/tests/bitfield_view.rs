//! `#[view]` — a contextual typed view over a bitfield's raw bits whose meaning can
//! depend on a *sibling* field. This is NXDN's LICH shape: the same two bits mean
//! different things depending on the direction bit stored alongside them. A bitfield
//! is random-access, so the view reads the sibling with no cursor look-ahead.

mod macro_ {
    use bnb::{bitfield, u2, u3};

    /// A value whose two wire bits are interpreted using a *direction* sibling.
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

    #[bitfield(u8, bits = msb)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Lich {
        header: u3,
        // The raw two bits are stored; `kind()` materializes the typed value using the
        // `outbound` sibling. The raw type (`u2`) is inferred from the closures.
        #[view(
            bits = 2,
            read = |raw: u2, s: &Self| Kind::interpret(raw, s.outbound()),
            write = |v: Kind| v.bits()
        )]
        kind: Kind,
        outbound: bool,
        trailing: u2,
    }

    #[test]
    fn view_reads_a_sibling_for_context_and_round_trips() {
        let l = Lich::new()
            .with_header(u3::new(0b101))
            .with_kind(Kind::A)
            .with_outbound(true)
            .with_trailing(u2::new(0b10));
        // The raw backing byte round-trips.
        assert_eq!(Lich::from_bytes(l.to_bytes()), l);
        // The view materializes the typed value from its bits and the sibling.
        assert_eq!(l.kind(), Kind::A); // outbound && bits 00
        assert_eq!(l.header(), u3::new(0b101));
        assert_eq!(l.trailing(), u2::new(0b10));
    }

    #[test]
    fn the_sibling_changes_what_the_same_bits_mean() {
        // Both store kind bits `00`; only `outbound` differs, so the view resolves a
        // different `kind` — the sibling drives the interpretation.
        let outbound = Lich::new().with_kind(Kind::A).with_outbound(true);
        let inbound = Lich::new()
            .with_kind(Kind::Reserved(u2::new(0b00)))
            .with_outbound(false);

        assert_eq!(outbound.kind(), Kind::A);
        assert_eq!(inbound.kind(), Kind::Reserved(u2::new(0b00)));
        // The raw bytes differ only in the single `outbound` bit.
        assert_eq!(
            outbound
                .to_raw()
                .count_ones()
                .abs_diff(inbound.to_raw().count_ones()),
            1
        );
    }

    #[test]
    fn set_in_place_writes_the_view_bits() {
        let mut l = Lich::new().with_outbound(false);
        l.set_kind(Kind::B); // B.bits() == 0b01
        assert_eq!(l.kind(), Kind::B); // inbound && 01 → B
    }

    #[test]
    fn debug_shows_the_typed_view() {
        let l = Lich::new().with_kind(Kind::A).with_outbound(true);
        // The intercepted Debug renders the logical getters, so `kind` shows the
        // resolved `Kind`, not the raw bits.
        assert!(format!("{l:?}").contains("kind: A"));
    }
}
