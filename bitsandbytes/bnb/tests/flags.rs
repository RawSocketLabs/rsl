//! `#[bitflags]` behavior: flag consts, set algebra, per-flag accessors,
//! iteration, dual-use retain, and composition (nesting in a `#[bitfield]`).
//! Codec-only, so it runs with and without the binrw feature.

mod macro_ {

    use bnb::{bitfield, bitflags, u4};

    #[bitflags(u8)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
    struct TcpFlags {
        fin: bool,
        syn: bool,
        rst: bool,
        psh: bool,
        ack: bool,
        urg: bool,
        ece: bool,
        cwr: bool,
    }

    impl TcpFlags {
        // A combination const, built with the generated const helpers.
        const HANDSHAKE: Self = Self::SYN.union(Self::ACK);
    }

    #[test]
    fn flag_consts_and_all_empty() {
        assert_eq!(TcpFlags::FIN.bits(), 0b0000_0001);
        assert_eq!(TcpFlags::SYN.bits(), 0b0000_0010);
        assert_eq!(TcpFlags::ACK.bits(), 0b0001_0000);
        assert_eq!(TcpFlags::CWR.bits(), 0b1000_0000);
        assert_eq!(TcpFlags::all().bits(), 0xFF);
        assert_eq!(TcpFlags::empty().bits(), 0);
        assert!(TcpFlags::empty().is_empty());
    }

    #[test]
    fn set_algebra_via_operators() {
        let f = TcpFlags::SYN | TcpFlags::ACK;
        assert_eq!(f.bits(), 0b0001_0010);
        assert!(f.contains(TcpFlags::SYN));
        assert!(f.contains(TcpFlags::HANDSHAKE));
        assert!(!f.contains(TcpFlags::FIN));
        assert!(f.intersects(TcpFlags::ACK));
        assert_eq!((f & TcpFlags::SYN), TcpFlags::SYN);
        assert_eq!((f - TcpFlags::SYN), TcpFlags::ACK);
        assert_eq!((f ^ TcpFlags::SYN), TcpFlags::ACK);
        assert_eq!(!TcpFlags::empty(), TcpFlags::all());
    }

    #[test]
    fn assign_operators_and_mutators() {
        let mut f = TcpFlags::empty();
        f |= TcpFlags::SYN;
        f.insert(TcpFlags::ACK);
        assert!(f.contains(TcpFlags::HANDSHAKE));
        f -= TcpFlags::SYN;
        assert!(!f.syn() && f.ack());
        f.toggle(TcpFlags::FIN);
        assert!(f.fin());
    }

    #[test]
    fn per_flag_accessors() {
        let mut f = TcpFlags::empty();
        assert!(!f.syn());
        f.set_syn(true);
        assert!(f.syn());
        let g = f.with_ack(true);
        assert!(g.ack() && g.syn());
        f.set_syn(false);
        assert!(!f.syn());
    }

    #[test]
    fn iter_yields_set_single_bit_flags_in_order() {
        let f = TcpFlags::SYN | TcpFlags::ACK | TcpFlags::FIN;
        let set: Vec<_> = f.iter().collect();
        // declaration order: fin (0), syn (1), ack (4).
        assert_eq!(set, vec![TcpFlags::FIN, TcpFlags::SYN, TcpFlags::ACK]);
        assert_eq!(TcpFlags::empty().iter().count(), 0);
    }

    // A small flag set with undefined high bits, to prove dual-use retain.
    #[bitflags(u8)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Few {
        a: bool,
        b: bool,
        #[flag(3)]
        c: bool, // pinned to bit 3, leaving bits 2/4..7 undefined
    }

    #[test]
    fn from_bits_retains_unknown_but_truncate_drops_them() {
        assert_eq!(Few::all().bits(), 0b0000_1011); // a|b|c
        // 0xFF has undefined bits set: retain keeps them, truncate drops them.
        assert_eq!(Few::from_bits(0xFF).bits(), 0xFF);
        assert_eq!(Few::from_bits_truncate(0xFF).bits(), 0b0000_1011);
        assert_eq!(Few::C.bits(), 0b0000_1000);
    }

    // Flags compose as a field inside a #[bitfield] (they implement Bits).
    #[bitfield(u16, bits = msb)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Frame {
        kind: u4,
        flags: TcpFlags, // an 8-bit flag set
        rsvd: u4,
    }

    #[test]
    fn flags_nest_in_a_bitfield() {
        let frame = Frame::new()
            .with_kind(u4::new(0xA))
            .with_flags(TcpFlags::SYN | TcpFlags::ACK);
        // kind in bits 12..=15, flags in 4..=11, rsvd in 0..=3.
        assert_eq!(frame.flags(), TcpFlags::SYN | TcpFlags::ACK);
        assert_eq!(
            frame.raw(),
            (0xA << 12) | ((TcpFlags::SYN | TcpFlags::ACK).bits() as u16) << 4
        );
        assert!(frame.flags().ack());
    }

    // Every backing width packs and round-trips — exercises the macro's per-backing byte
    // arithmetic (the `u8` cases above only cover the 1-byte path).
    #[bitflags(u16)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct W16 {
        a: bool,
        #[flag(15)]
        top: bool,
    }

    #[bitflags(u32)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct W32 {
        a: bool,
        #[flag(31)]
        top: bool,
    }

    #[bitflags(u64)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct W64 {
        a: bool,
        #[flag(63)]
        top: bool,
    }

    #[bitflags(u128)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct W128 {
        a: bool,
        #[flag(127)]
        top: bool,
    }

    #[test]
    fn every_backing_width_places_its_top_bit() {
        assert_eq!(W16::TOP.bits(), 1u16 << 15);
        assert_eq!(W32::TOP.bits(), 1u32 << 31);
        assert_eq!(W64::TOP.bits(), 1u64 << 63);
        assert_eq!(W128::TOP.bits(), 1u128 << 127);
        // The set operators and accessors work the same at every width.
        let f = W32::A | W32::TOP;
        assert!(f.a() && f.top());
        assert_eq!((f - W32::A), W32::TOP);
    }
}
