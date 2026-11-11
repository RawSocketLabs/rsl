//! A type that reaches the macros through a `macro_rules!` `$t:ty` fragment arrives wrapped
//! in an invisible group. The macros read a field's type by its shape (a bare `bool`/`u8`,
//! a `Vec<_>`, an `Option<_>`, a `[u8; N]`), so they must look through that group.

mod macro_ {
    use bnb::{BitEnum, bin, bitfield, bitflags, u7};

    macro_rules! packed {
        ($name:ident, $flag:ty, $num:ty) => {
            #[bitfield(u16)]
            #[derive(Clone, Copy, Debug, PartialEq, Eq)]
            struct $name {
                on: $flag,
                value: $num,
                rest: u7,
            }
        };
    }
    packed!(Packed, bool, u8);

    macro_rules! flag_set {
        ($name:ident, $flag:ty) => {
            #[bitflags(u8)]
            #[derive(Clone, Copy, Debug, PartialEq, Eq)]
            struct $name {
                first: $flag,
                second: $flag,
            }
        };
    }
    flag_set!(Flags, bool);

    macro_rules! code {
        ($name:ident, $width:ty) => {
            #[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
            #[bit_enum($width)]
            enum $name {
                Zero,
                One,
                #[catch_all]
                Other($width),
            }
        };
    }
    code!(Code, u8);

    macro_rules! message {
        ($name:ident, $array:ty, $list:ty, $optional:ty) => {
            #[bin(big)]
            #[derive(Debug, PartialEq)]
            struct $name {
                head: $array,
                n: u8,
                #[br(count = n)]
                items: $list,
                #[br(if(n > 1))]
                extra: $optional,
            }
        };
    }
    message!(Message, [u8; 2], Vec<u8>, Option<u8>);

    #[test]
    fn a_bitfield_recognises_primitives_passed_as_type_fragments() {
        let packed = Packed::new().with_on(true).with_value(0xAB);
        assert!(packed.on());
        assert_eq!(packed.value(), 0xAB);
        // on(1) | value(1010_1011) | rest(000_0000), msb-first.
        assert_eq!(packed.to_raw(), 0xD580);
    }

    #[test]
    fn bitflags_and_bit_enum_recognise_type_fragments() {
        assert_eq!((Flags::FIRST | Flags::SECOND).bits(), 0b11);
        // A primitive width gets the `From` conversions.
        assert_eq!(Code::from(1u8), Code::One);
        assert_eq!(u8::from(Code::Other(9)), 9);
    }

    #[test]
    fn a_message_recognises_field_shapes_passed_as_type_fragments() {
        let bytes = [0xAA, 0xBB, 2, 1, 2, 9];
        let message = Message::decode_exact(&bytes).unwrap();
        assert_eq!(
            message,
            Message {
                head: [0xAA, 0xBB],
                n: 2,
                items: vec![1, 2],
                extra: Some(9),
            }
        );
        assert_eq!(message.to_bytes().unwrap(), bytes);
    }
}
