//! Edge-case / robustness stress tests for `#[wire]`.
#![cfg(feature = "binrw")]

use binrw::{BinRead, BinWrite};
use bnb::{bitfield, u4, wire};
use std::io::Cursor;

/// Write `$v`, read it back as `$ty`, assert equality, and return the bytes.
macro_rules! rt {
    ($ty:ty, $v:expr) => {{
        let v: $ty = $v;
        let mut buf = Cursor::new(Vec::new());
        v.write(&mut buf).unwrap();
        let bytes = buf.into_inner();
        let back = <$ty>::read(&mut Cursor::new(&bytes)).unwrap();
        assert_eq!(back, v, "round-trip mismatch");
        bytes
    }};
}

// 1. Little-endian message with a multi-byte group word.
#[wire(little, group(hi, lo => u16))]
#[derive(Debug, Clone, PartialEq)]
struct LeGroup {
    hi: u8,
    lo: u8,
    tail: u16,
}

#[test]
fn le_multibyte_group() {
    let m = LeGroup {
        hi: 0x12,
        lo: 0x34,
        tail: 0x5678,
    };
    let bytes = rt!(LeGroup, m);
    // group value 0x1234 (hi high byte, lo low byte) serialized little-endian,
    // then tail 0x5678 little-endian.
    assert_eq!(bytes, vec![0x34, 0x12, 0x78, 0x56]);
}

// 2. Back-to-back groups with no field between them.
#[wire(big, group(a, b => u8), group(c, d => u8))]
#[derive(Debug, Clone, PartialEq)]
struct BackToBack {
    a: u4,
    b: u4,
    c: u4,
    d: u4,
}

#[test]
fn back_to_back_groups() {
    let m = BackToBack {
        a: u4::new(0x1),
        b: u4::new(0x2),
        c: u4::new(0x3),
        d: u4::new(0x4),
    };
    assert_eq!(rt!(BackToBack, m), vec![0x12, 0x34]);
}

// 3. A nested #[bitfield] used as a group member.
#[bitfield(u8, bits = msb, bytes = be)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Sub {
    x: u4,
    y: u4,
}

#[wire(big, group(flag, sub => u16))]
#[derive(Debug, Clone, PartialEq)]
struct NestedBitfieldMember {
    flag: u8,
    sub: Sub,
}

#[test]
fn nested_bitfield_group_member() {
    let m = NestedBitfieldMember {
        flag: 0xAB,
        sub: Sub::new().with_x(u4::new(0xC)).with_y(u4::new(0xD)),
    };
    assert_eq!(rt!(NestedBitfieldMember, m), vec![0xAB, 0xCD]);
}

// 4. A group that exactly fills a u16 from two members of explicit widths
//    (members must account for every bit of the backing — see the compile-fail
//    `group_underfilled` case for the rejection of a short group).
#[wire(big, group(hi, lo => u16))]
#[derive(Debug, Clone, PartialEq)]
struct FullWidth {
    hi: u8,
    lo: u8,
}

#[test]
fn full_width_group() {
    let m = FullWidth { hi: 0xAB, lo: 0xCD };
    assert_eq!(rt!(FullWidth, m), vec![0xAB, 0xCD]);
}

// 5. builder_only without a default (required in the builder).
#[wire(big)]
#[derive(Debug, Clone, PartialEq)]
struct BuilderOnlyRequired {
    id: u16,
    #[builder_only]
    note: u8,
}

#[test]
fn builder_only_required() {
    // Required in the builder...
    let err = BuilderOnlyRequired::builder().id(1).build().unwrap_err();
    assert_eq!(err.field(), Some("note"));
    let m = BuilderOnlyRequired::builder()
        .id(1)
        .note(9)
        .build()
        .unwrap();
    let mut buf = Cursor::new(Vec::new());
    m.write(&mut buf).unwrap();
    assert_eq!(buf.get_ref().as_slice(), &[0x00, 0x01]); // note not on wire
    // ...defaults via Default on read.
    let back = BuilderOnlyRequired::read(&mut Cursor::new([0x00, 0x01])).unwrap();
    assert_eq!(back.note, 0);
}

// 7. A user-declared `check_soundness` field (the NBT pattern): the macro must
//    NOT inject a second one.
fn ud_check(s: &UserDeclaredFlag) -> Result<(), String> {
    if s.id == 0 { Err("bad".into()) } else { Ok(()) }
}

#[wire(big, validate = ud_check)]
#[derive(Debug, Clone, PartialEq)]
struct UserDeclaredFlag {
    id: u16,
    #[builder_only(default = true)]
    check_soundness: bool,
}

#[test]
fn user_declared_check_soundness() {
    assert!(UserDeclaredFlag::builder().id(0).build().is_err());
    let ok = UserDeclaredFlag::builder().id(5).build().unwrap();
    assert_eq!(ok.id, 5);
    // explicit opt-out still works
    let bad = UserDeclaredFlag::builder()
        .id(0)
        .check_soundness(false)
        .build()
        .unwrap();
    assert_eq!(bad.id, 0);
}

// 8. validate + no_builder: the validate() method exists even without build().
fn nb_check(s: &ValidateNoBuilder) -> Result<(), String> {
    if s.id == 0 {
        Err("zero".into())
    } else {
        Ok(())
    }
}

#[wire(big, no_builder, validate = nb_check)]
#[derive(Debug, Clone, PartialEq)]
struct ValidateNoBuilder {
    id: u16,
    #[builder_only(default = true)]
    check_soundness: bool,
}

#[test]
fn validate_without_builder() {
    let good = ValidateNoBuilder {
        id: 1,
        check_soundness: true,
    };
    assert!(good.validate().is_ok());
    let bad = ValidateNoBuilder {
        id: 0,
        check_soundness: true,
    };
    assert!(bad.validate().is_err());
}

// 9. A custom (non-String) Display error type from the validator.
#[derive(Debug)]
struct MyErr(u16);
impl std::fmt::Display for MyErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "bad id {}", self.0)
    }
}
fn custom_err_check(s: &CustomErr) -> Result<(), MyErr> {
    if s.id == 0 { Err(MyErr(s.id)) } else { Ok(()) }
}

#[wire(big, validate = custom_err_check)]
#[derive(Debug, Clone, PartialEq)]
struct CustomErr {
    id: u16,
}

#[test]
fn custom_display_error() {
    let err = CustomErr::builder().id(0).build().unwrap_err();
    assert_eq!(err.to_string(), "soundness check failed: bad id 0");
}

// 10. A #[wire] struct nested inside another #[wire] struct.
#[wire(big, group(a, b => u8))]
#[derive(Debug, Clone, PartialEq)]
struct Inner {
    a: u4,
    b: u4,
    val: u16,
}

#[wire(big)]
#[derive(Debug, Clone, PartialEq)]
struct Outer {
    tag: u8,
    inner: Inner,
}

#[test]
fn wire_within_wire() {
    let m = Outer {
        tag: 0xFF,
        inner: Inner {
            a: u4::new(0x1),
            b: u4::new(0x2),
            val: 0xABCD,
        },
    };
    assert_eq!(rt!(Outer, m), vec![0xFF, 0x12, 0xAB, 0xCD]);
}

// 11. Two structs with identically-named group members (type names must not
//     collide — they are namespaced by struct name).
#[wire(big, group(a, b => u8))]
#[derive(Debug, Clone, PartialEq)]
struct First {
    a: u4,
    b: u4,
}
#[wire(big, group(a, b => u8))]
#[derive(Debug, Clone, PartialEq)]
struct Second {
    a: u4,
    b: u4,
}

#[test]
fn group_type_names_do_not_collide() {
    let f = First {
        a: u4::new(1),
        b: u4::new(2),
    };
    let s = Second {
        a: u4::new(3),
        b: u4::new(4),
    };
    assert_eq!(rt!(First, f), vec![0x12]);
    assert_eq!(rt!(Second, s), vec![0x34]);
}
