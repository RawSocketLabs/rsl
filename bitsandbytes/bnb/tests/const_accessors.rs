//! Const-context proof of the generated accessors and the `repr(transparent)`
//! layout guarantee.
//!
//! Every getter/`with_*`/`set_*` here is exercised inside a `const` item or
//! block, which is the only real proof of const-ness — a passing runtime call
//! proves nothing. The runtime tests then re-check the same values so a
//! const-eval shortcut can't diverge from the runtime path, and lock the packed
//! layouts (the const rework must not move a single bit).

mod macro_ {

    use bnb::{BitEnum, bitfield, bitflags, u1, u2, u3, u4, u7, u20, u24, u127};
    use core::mem::{align_of, size_of};

    // ---------------------------------------------------------------------------
    // Odd widths (u1/u2/u7/u20/u24), msb, and const setters.
    // ---------------------------------------------------------------------------

    #[bitfield(u32, bits = msb)]
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    struct Odd {
        a: u1,
        b: u7,
        c: u20,
        d: u2,
        e: u2,
    }

    const ODD: Odd = Odd::new()
        .with_a(u1::new(1))
        .with_b(u7::new(0x55))
        .with_c(u20::new(0xABCDE))
        .with_d(u2::new(3))
        .with_e(u2::new(0));

    const _: () = {
        assert!(ODD.a().value() == 1);
        assert!(ODD.b().value() == 0x55);
        assert!(ODD.c().value() == 0xABCDE);
        assert!(ODD.d().value() == 3);
        assert!(ODD.e().value() == 0);
        // In-place setters work on a `mut` binding in const eval.
        let mut m = ODD;
        m.set_c(u20::new(1));
        assert!(m.c().value() == 1);
        assert!(m.a().value() == 1); // neighbors untouched
        assert!(m.d().value() == 3);
    };

    #[test]
    fn odd_widths_layout_is_unchanged() {
        // a@31 | b@24 | c@4 | d@2 | e@0 — the exact packing from before the
        // const rework.
        assert_eq!(ODD.to_raw(), 0xD5AB_CDEC);
        assert_eq!(Odd::from_raw(0xD5AB_CDEC), ODD);
    }

    // ---------------------------------------------------------------------------
    // u24, lsb order, and manual `#[bits(A..=B)]` ranges.
    // ---------------------------------------------------------------------------

    #[bitfield(u32, bits = lsb)]
    #[derive(Clone, Copy)]
    struct Lsb {
        lo: u24,
        flag: bool,
        rest: u7,
    }

    const LSB: Lsb = Lsb::new().with_lo(u24::new(0xBEEF42)).with_flag(true);
    const _: () = {
        assert!(LSB.lo().value() == 0xBEEF42);
        assert!(LSB.flag());
        assert!(LSB.rest().value() == 0);
    };

    #[bitfield(u8)]
    #[derive(Clone, Copy)]
    struct Ranged {
        #[bits(0..=3)]
        lo: u4,
        #[bits(4..=7)]
        hi: u4,
    }

    const RANGED: Ranged = Ranged::new().with_lo(u4::new(0xA)).with_hi(u4::new(0x5));
    const _: () = {
        assert!(RANGED.lo().value() == 0xA);
        assert!(RANGED.hi().value() == 0x5);
    };

    #[test]
    fn lsb_and_ranged_layouts_are_unchanged() {
        assert_eq!(LSB.to_raw(), 0x01BE_EF42);
        assert_eq!(RANGED.to_raw(), 0x5A);
    }

    // ---------------------------------------------------------------------------
    // Enum fields: exhaustive, catch-all (materialized in const), and bool.
    // ---------------------------------------------------------------------------

    #[derive(BitEnum, Clone, Copy, PartialEq, Eq, Debug)]
    #[bit_enum(u2)]
    enum Quad {
        N = 0,
        E = 1,
        S = 2,
        W = 3,
    }

    #[derive(BitEnum, Clone, Copy, PartialEq, Eq, Debug)]
    #[bit_enum(u3)]
    enum Mode {
        // Auto-numbered from 0 (a catch-all forbids explicit discriminants
        // without a `#[repr]` — the documented gotcha).
        Idle,
        Run,
        #[catch_all]
        Other(u3),
    }

    #[bitfield(u8, bits = msb)]
    #[derive(Clone, Copy)]
    struct Ctl {
        quad: Quad,
        on: bool,
        mode: Mode,
        pad: u2,
    }

    const CTL: Ctl = Ctl::new()
        .with_quad(Quad::S)
        .with_on(true)
        .with_mode(Mode::Run)
        .with_pad(u2::new(0));

    const _: () = {
        assert!(matches!(CTL.quad(), Quad::S));
        assert!(CTL.on());
        assert!(matches!(CTL.mode(), Mode::Run));
        // An unknown discriminant materializes the catch-all variant in const eval.
        let unknown = CTL.with_mode(Mode::Other(u3::new(0b110)));
        assert!(matches!(unknown.mode(), Mode::Other(v) if v.value() == 0b110));
    };

    #[test]
    fn enum_fields_round_trip_at_runtime() {
        assert_eq!(CTL.quad(), Quad::S);
        assert_eq!(CTL.mode(), Mode::Run);
        // quad@6 (S=2 → 10) | on@5 (1) | mode@2 (Run → 001) | pad@0 (00)
        assert_eq!(CTL.to_raw(), 0b1010_0100);
    }

    // ---------------------------------------------------------------------------
    // Nested bitfield and nested flag-set fields read const through two levels.
    // ---------------------------------------------------------------------------

    #[bitfield(u8, bits = msb)]
    #[derive(Clone, Copy)]
    struct Inner {
        hi: u4,
        lo: u4,
    }

    #[bitflags(u8)]
    #[derive(Clone, Copy)]
    struct Fl {
        ready: bool,
        error: bool,
    }

    #[bitfield(u32, bits = msb)]
    #[derive(Clone, Copy)]
    struct Outer {
        inner: Inner,
        flags: Fl,
        tail: u16,
    }

    const OUTER: Outer = Outer::new()
        .with_inner(Inner::new().with_hi(u4::new(0xC)).with_lo(u4::new(0x3)))
        .with_flags(Fl::empty().with_ready(true))
        .with_tail(0xBEEF);

    const _: () = {
        assert!(OUTER.inner().hi().value() == 0xC);
        assert!(OUTER.inner().lo().value() == 0x3);
        assert!(OUTER.flags().ready());
        assert!(!OUTER.flags().error());
        assert!(OUTER.tail() == 0xBEEF);
        // Flag-set mutators are const too.
        let mut f = OUTER.flags();
        f.insert(Fl::ERROR);
        f.set(Fl::READY, false);
        assert!(f.error() && !f.ready());
    };

    #[test]
    fn nested_layout_is_unchanged() {
        assert_eq!(OUTER.to_raw(), 0xC301_BEEF);
    }

    // ---------------------------------------------------------------------------
    // Full-width and 127-bit fields on a u128 backing (the mask edge cases).
    // ---------------------------------------------------------------------------

    #[bitfield(u128)]
    #[derive(Clone, Copy)]
    struct Full {
        all: u128,
    }

    #[bitfield(u128, bits = msb)]
    #[derive(Clone, Copy)]
    struct Split {
        head: u1,
        rest: u127,
    }

    const FULL: Full = Full::new().with_all(u128::MAX - 1);
    const SPLIT: Split = Split::new().with_head(u1::new(1)).with_rest(u127::new(7));
    const _: () = {
        assert!(FULL.all() == u128::MAX - 1);
        assert!(SPLIT.head().value() == 1);
        assert!(SPLIT.rest().value() == 7);
    };

    #[test]
    fn wide_layouts_are_unchanged() {
        assert_eq!(FULL.to_raw(), u128::MAX - 1);
        assert_eq!(SPLIT.to_raw(), (1u128 << 127) | 7);
    }

    // ---------------------------------------------------------------------------
    // Layout guarantee: a generated struct is repr(transparent) over its backing.
    // ---------------------------------------------------------------------------

    const _: () = {
        assert!(size_of::<Inner>() == size_of::<u8>() && align_of::<Inner>() == align_of::<u8>());
        assert!(size_of::<Ctl>() == size_of::<u8>() && align_of::<Ctl>() == align_of::<u8>());
        assert!(size_of::<Odd>() == size_of::<u32>() && align_of::<Odd>() == align_of::<u32>());
        assert!(size_of::<Outer>() == size_of::<u32>() && align_of::<Outer>() == align_of::<u32>());
        assert!(size_of::<Full>() == size_of::<u128>() && align_of::<Full>() == align_of::<u128>());
        // Flag sets get the same guarantee.
        assert!(size_of::<Fl>() == size_of::<u8>() && align_of::<Fl>() == align_of::<u8>());
    };

    // A u16 and a u64 backing, so every backing width is covered.
    #[bitfield(u16, bits = msb)]
    #[derive(Clone, Copy)]
    struct Half {
        a: u8,
        b: u8,
    }

    #[bitfield(u64, bits = msb)]
    #[derive(Clone, Copy)]
    struct Wide {
        a: u32,
        b: u32,
    }

    const _: () = {
        assert!(size_of::<Half>() == size_of::<u16>() && align_of::<Half>() == align_of::<u16>());
        assert!(size_of::<Wide>() == size_of::<u64>() && align_of::<Wide>() == align_of::<u64>());
        // Primitive-typed fields read/write const too.
        let h = Half::new().with_a(0x12).with_b(0x34);
        assert!(h.a() == 0x12 && h.b() == 0x34);
    };

    // A user-supplied `#[repr(...)]` wins — the macro then emits none of its own
    // (`repr(C)` and `repr(transparent)` cannot combine).
    #[bitfield(u8, bits = msb)]
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct UserRepr {
        hi: u4,
        lo: u4,
    }

    #[test]
    fn user_supplied_repr_is_respected() {
        // With repr(C) on a single-int struct the size still matches; the point
        // is that the item compiles (two reprs would not).
        assert_eq!(size_of::<UserRepr>(), 1);
        let u = UserRepr::new().with_hi(u4::new(0xF));
        assert_eq!(u.to_raw(), 0xF0);
    }
}
