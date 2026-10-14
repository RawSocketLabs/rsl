//! `#[bitfield]` emits `From` in both directions over its backing integer, matching
//! `to_raw`/`from_raw`: total, unvalidated, and usable as a `From`/`Into` bound.

mod macro_ {
    use bnb::{bitfield, u3, u4};

    #[bitfield(u8)]
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    struct Byte {
        #[bits(4..=7)]
        hi: u4,
        #[bits(0..=3)]
        lo: u4,
    }

    /// Declared narrower than its backing: the reserved bits still round-trip.
    #[bitfield(u16)]
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    struct Narrow {
        #[bits(0..=2)]
        low: u3,
    }

    fn round_trip<T: From<u8> + Into<u8>>(raw: u8) -> u8 {
        T::from(raw).into()
    }

    #[test]
    fn from_matches_raw_conversions() {
        let b = Byte::from(0xab);
        assert_eq!(b, Byte::from_raw(0xab));
        assert_eq!(b.hi().value(), 0xa);
        assert_eq!(b.lo().value(), 0xb);
        assert_eq!(u8::from(b), b.to_raw());
    }

    #[test]
    fn from_is_total_and_unvalidated() {
        for raw in 0..=u8::MAX {
            assert_eq!(round_trip::<Byte>(raw), raw);
        }
        let n = Narrow::from(0xfff5);
        assert_eq!(n.low().value(), 5);
        assert_eq!(
            u16::from(n),
            0xfff5,
            "bits beyond WIDTH are kept, as with from_raw"
        );
    }
}
