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
        // `const`, so the generated view accessors (which inline the closure
        // bodies) can themselves be `const fn`. A non-`const` helper needs the
        // view's `dynamic` opt-out instead — see `dynamic_view_calls_closures`.
        const fn interpret(bits: u2, outbound: bool) -> Self {
            match (outbound, bits.value()) {
                (true, 0b00) => Kind::A,
                (false, 0b01) => Kind::B,
                _ => Kind::Reserved(bits),
            }
        }

        const fn bits(self) -> u2 {
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
        // `const` *asserts* const accessors (the annotations alone already enable
        // them; the assertion turns any quiet fallback into a compile error).
        #[view(
            bits = 2,
            const,
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

    #[test]
    fn annotated_view_accessors_are_const() {
        // The whole build-and-read path in const eval — the only real proof of
        // const-ness (a runtime call proves nothing).
        const L: Lich = Lich::new()
            .with_header(u3::new(0b101))
            .with_kind(Kind::A)
            .with_outbound(true);
        const KIND: Kind = L.kind();
        assert_eq!(KIND, Kind::A);
        const _: () = {
            let mut l = Lich::new().with_outbound(false);
            l.set_kind(Kind::B);
            assert!(matches!(l.kind(), Kind::B));
        };
    }

    // A helper the const dispatch cannot inline into a `const fn` (it isn't one).
    fn parity(bits: u2) -> bool {
        bits.value().count_ones() % 2 == 1
    }

    // `dynamic` opts a view out of `const` accessors: the closures are called at
    // runtime, so they may use non-`const` operations.
    #[bitfield(u8, bits = msb)]
    #[derive(Clone, Copy)]
    struct DynLich {
        header: u3,
        #[view(
            bits = 2,
            dynamic,
            read = |raw: u2, _s: &Self| parity(raw),
            write = |v: bool| u2::new(v as u8)
        )]
        odd: bool,
        pad: u3,
    }

    #[test]
    fn dynamic_view_calls_closures() {
        let d = DynLich::new().with_odd(true);
        assert!(d.odd()); // stored 0b01 → one bit set → odd parity
    }

    // No `raw =`, no closure annotations: the raw type is invisible to the const
    // dispatch, so the accessors quietly keep the closure-calling (non-`const`)
    // form and the raw type is inferred — the 0.3.1 behavior.
    #[bitfield(u8, bits = msb)]
    #[derive(Clone, Copy)]
    struct Unannotated {
        header: u3,
        #[view(
            bits = 2,
            read = |raw, _s| Kind::interpret(raw, true),
            write = |v: Kind| v.bits()
        )]
        kind: Kind,
        pad: u3,
    }

    #[test]
    fn unannotated_read_still_infers() {
        let u = Unannotated::new().with_kind(Kind::A);
        assert_eq!(u.kind(), Kind::A);
    }

    // `read`/`write` as *paths to const fns* (not closures) plus `raw = <ty>` —
    // the direct-call const form: paths carry no annotation, so `raw` is the only
    // way the macro can see the stored type.
    const fn path_read(raw: u2, s: &PathView) -> Kind {
        Kind::interpret(raw, s.outbound())
    }

    const fn path_write(v: Kind) -> u2 {
        v.bits()
    }

    #[bitfield(u8, bits = msb)]
    #[derive(Clone, Copy)]
    struct PathView {
        header: u3,
        #[view(bits = 2, const, raw = u2, read = path_read, write = path_write)]
        kind: Kind,
        outbound: bool,
        pad: u2,
    }

    #[test]
    fn path_fns_with_raw_key_are_const() {
        const P: PathView = PathView::new().with_kind(Kind::B).with_outbound(false);
        const KIND: Kind = P.kind();
        assert_eq!(KIND, Kind::B); // inbound && bits 01 → B
        const _: () = {
            let mut p = P;
            p.set_kind(Kind::Reserved(u2::new(0b11)));
            assert!(matches!(p.kind(), Kind::Reserved(v) if v.value() == 0b11));
        };
    }

    // The `write` closure's *return annotation* as the only raw-type source (the
    // third resolution rule, after `raw =` and the `read` param annotation).
    #[bitfield(u8, bits = msb)]
    #[derive(Clone, Copy)]
    struct WriteAnnotated {
        header: u3,
        #[view(
            bits = 2,
            const,
            read = |raw, _s| raw,
            write = |v: u2| -> u2 { v }
        )]
        val: u2,
        pad: u3,
    }

    #[test]
    fn write_return_annotation_resolves_the_raw_type() {
        const W: WriteAnnotated = WriteAnnotated::new().with_val(u2::new(0b10));
        const V: u2 = W.val();
        assert_eq!(V, u2::new(0b10));
    }

    // A `write` body with an early `return`: inlining it would turn "map the
    // value" into "skip the store", so the store conservatively keeps the
    // closure-calling (non-`const`) form — closure semantics preserved. The
    // getter (no `return` concern) stays const.
    #[bitfield(u8, bits = msb)]
    #[derive(Clone, Copy)]
    struct RetWrite {
        header: u3,
        #[view(
            bits = 2,
            read = |raw: u2, _s: &Self| raw,
            write = |v: u2| { if v.value() == 3 { return u2::new(0); } v }
        )]
        clamped: u2,
        pad: u3,
    }

    #[test]
    fn write_body_with_return_keeps_closure_semantics() {
        // The early `return` exits the closure, not the setter: the store runs.
        let clamped = RetWrite::new().with_clamped(u2::new(3));
        assert_eq!(clamped.clamped(), u2::new(0));
        let plain = RetWrite::new().with_clamped(u2::new(2));
        assert_eq!(plain.clamped(), u2::new(2));
        // The getter is still const (its inlining is unaffected by `return`).
        const G: u2 = RetWrite::new().clamped();
        assert_eq!(G, u2::new(0));
    }
}
