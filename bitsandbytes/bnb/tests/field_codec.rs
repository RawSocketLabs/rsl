//! The unified field-codec: one `T: BitDecode` bound decodes a **primitive**, a **bitfield**,
//! and a **message** through the *same* call — the basis for `#[bin]` needing no `#[nested]`.
//! The `Bits` leaves' impls are now auto-emitted by their macros (no hand-written impl here),
//! while `Bits` (packing) is untouched.

use bnb::{BitDecode, BitReader, bin, bitfield, u4, u12};

#[bitfield(u16, bits = msb)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Flags {
    hi: u4,
    lo: u12,
}

#[bin(big)]
#[derive(Debug, PartialEq)]
struct Msg {
    a: u16,
}

/// One generic over `BitDecode` — works for *any* field type, leaf or message, no marker.
fn read_field<T: BitDecode>(bytes: &[u8]) -> T {
    T::bit_decode(&mut BitReader::new(bytes)).unwrap()
}

#[test]
fn one_path_for_primitive_bitfield_and_message() {
    let n: u16 = read_field(&[0x12, 0x34]); // primitive Bits leaf  (runtime impl)
    let f: Flags = read_field(&[0xAB, 0xCD]); // bitfield Bits leaf   (macro-emitted)
    let m: Msg = read_field(&[0x56, 0x78]); // message               (the #[bin] derive)

    assert_eq!(n, 0x1234);
    assert_eq!(
        f,
        Flags::new().with_hi(u4::new(0xA)).with_lo(u12::new(0xBCD))
    );
    assert_eq!(m, Msg { a: 0x5678 });

    // And `Flags` still packs as a `Bits` value — the packing role is untouched.
    assert_eq!(f.to_be_bytes(), [0xAB, 0xCD]);
}

// Backward compatibility: `#[nested]` is obsolete but still **accepted** (a no-op), so existing
// code keeps compiling. The same message decodes whether the marker is present or not.
#[bin(big)]
#[derive(Debug, PartialEq)]
struct Outer {
    tag: u8,
    #[br(temp)]
    #[bw(calc = self.items.len() as u8)]
    count: u8,
    #[br(count = count)]
    #[nested] // tolerated, ignored
    items: Vec<Msg>,
}

#[test]
fn nested_marker_is_tolerated_as_a_noop() {
    let o = Outer {
        tag: 9,
        items: vec![Msg { a: 0x1111 }, Msg { a: 0x2222 }],
    };
    assert_eq!(Outer::decode_exact(&o.to_bytes().unwrap()).unwrap(), o);
}
