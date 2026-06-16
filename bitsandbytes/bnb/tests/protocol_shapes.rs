//! Real-world validation: reproduce the exact bit/byte layouts the workspace's
//! protocol crates implement today with `bitbybit` / `modular-bitfield(-msb)`,
//! and prove `bits` produces the same wire bytes. These are the production
//! use-cases the crate must cover.

use bnb::{BitEnum, bitfield, u3, u4, u5};

// ---------------------------------------------------------------------------
// DNS message header `State` (application/dns/src/message/{op,state}.rs).
//
// A 16-bit, big-endian, MSB-first field collapsing opcode/flags/rcode:
//
//   bit 15 .............. bit 0
//   | opcode(5) | flags(7) | rcode(4) |
//
// `OpCode` is itself a 5-bit bitfield (response bit + 4-bit Op); `RCode` is a
// 4-bit catch-all enum.
// ---------------------------------------------------------------------------

#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[bit_enum(u4)]
enum Op {
    #[default]
    Query = 0,
    Inverse = 1,
    Status = 2,
    Update = 5,
    // Exhaustive: every other 4-bit value is named (so no #[catch_all]).
    R3 = 3,
    R4 = 4,
    R6 = 6,
    R7 = 7,
    R8 = 8,
    R9 = 9,
    R10 = 10,
    R11 = 11,
    R12 = 12,
    R13 = 13,
    R14 = 14,
    R15 = 15,
}

#[bitfield(u8, bits = msb)] // 5 used bits: response in the high bit, op in the low 4
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OpCode {
    response: bool,
    op: Op,
}

#[bitfield(u8, bits = msb)] // 7 used bits
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Flags {
    authoritative: bool,
    truncated: bool,
    recursion_desired: bool,
    recursion_available: bool,
    reserved: u3,
}

// With a catch-all (tuple) variant, Rust forbids explicit discriminants unless
// the enum is `#[repr(..)]`. For contiguous-from-0 values the derive's
// auto-numbering (matching Rust's own) gives NoError=0, FormErr=1, …; only a
// non-contiguous catch-all enum needs `#[repr(u8)]` + explicit discriminants.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u4)]
enum RCode {
    NoError,
    FormErr,
    ServFail,
    NxDomain,
    #[catch_all]
    Other(u4), // dual-use: unknown rcodes preserved
}

#[bitfield(u16, bits = msb, bytes = be)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct State {
    opcode: OpCode,
    flags: Flags,
    rcode: RCode,
}

#[test]
fn dns_state_bit_positions_match_the_wire() {
    // opcode.op = Status(2), no response; rcode = ServFail(2); flags = 0.
    let state = State::new()
        .with_opcode(OpCode::new().with_op(Op::Status))
        .with_rcode(RCode::ServFail);

    // opcode occupies bits 11..=15, so op=2 lands at 2 << 11 = 0x1000;
    // rcode occupies bits 0..=3, so 2. Total 0x1002.
    assert_eq!(state.raw(), 0x1002);
    assert_eq!(state.to_be_bytes(), [0x10, 0x02]);

    // Getters round-trip through the nested types.
    assert_eq!(state.opcode().op(), Op::Status);
    assert!(!state.opcode().response());
    assert_eq!(state.rcode(), RCode::ServFail);
}

#[test]
fn dns_state_nested_flags_land_in_the_right_bits() {
    // AA (authoritative) is DNS header bit 10; in our layout flags occupy bits
    // 4..=10, and authoritative is the high bit of the 7-bit flags field (bit 6
    // of flags) -> state bit 4 + 6 = 10.
    let state = State::new().with_flags(Flags::new().with_authoritative(true));
    assert_eq!(state.raw(), 1 << 10);
    assert!(state.flags().authoritative());

    let state = State::new().with_flags(Flags::new().with_recursion_desired(true));
    // recursion_desired is the 3rd flag bit (bit 4 of flags) -> state bit 4+4=8.
    assert_eq!(state.raw(), 1 << 8);
}

#[test]
fn dns_rcode_catch_all_preserves_unknown_values() {
    // 0xF is not a named RCode; it must round-trip through the catch-all.
    let state = State::from_raw(0x000F);
    assert_eq!(state.rcode(), RCode::Other(u4::new(0xF)));
    // And re-encoding yields the same bits.
    assert_eq!(state.with_rcode(RCode::Other(u4::new(0xF))).raw(), 0x000F);
}

#[test]
fn dns_op_is_exhaustive_and_round_trips_all_16() {
    for v in 0u128..16 {
        let op = <Op as bnb::Bits>::from_bits(v);
        assert_eq!(<Op as bnb::Bits>::into_bits(op), v, "op {v}");
    }
}

// ---------------------------------------------------------------------------
// SMB `SecurityMode` (application/smb/.../security_mode.rs): little-endian.
// A single byte here, but we also exercise a multi-byte little-endian field to
// prove byte order is honored.
// ---------------------------------------------------------------------------

#[bitfield(u8, bits = lsb)] // SMB packs LSB-first
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SecurityMode {
    user_mode: bool,
    encrypt_passwords: bool,
    signing_enabled: bool,
    signing_required: bool,
    reserved: u4,
}

#[bitfield(u32, bits = lsb, bytes = le)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Capabilities {
    raw_mode: bool,
    mpx_mode: bool,
    unicode: bool,
    large_files: bool,
    rest: bnb::u28,
}

#[test]
fn smb_security_mode_lsb_first() {
    let mode = SecurityMode::new()
        .with_user_mode(true)
        .with_signing_required(true);
    // LSB-first: user_mode = bit 0, signing_required = bit 3.
    assert_eq!(mode.raw(), 0b0000_1001);
    assert!(mode.user_mode());
    assert!(mode.signing_required());
    assert!(!mode.signing_enabled());
}

#[test]
fn smb_capabilities_little_endian_bytes() {
    let caps = Capabilities::new().with_unicode(true); // bit 2
    assert_eq!(caps.raw(), 0x0000_0004);
    // Little-endian: the low byte (0x04) comes first.
    assert_eq!(caps.to_le_bytes(), [0x04, 0x00, 0x00, 0x00]);
    assert_eq!(caps.to_be_bytes(), [0x00, 0x00, 0x00, 0x04]);
}

// ---------------------------------------------------------------------------
// Manual (range) layout — the bitbybit `#[bits(11..=15)]` style, an escape
// hatch for absolute control.
// ---------------------------------------------------------------------------

#[bitfield(u16, bytes = be)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ManualState {
    #[bits(11..=15)]
    opcode: u5,
    #[bits(4..=10)]
    flags: bnb::u7,
    #[bits(0..=3)]
    rcode: u4,
}

#[test]
fn manual_ranges_give_absolute_offsets() {
    let s = ManualState::new()
        .with_opcode(u5::new(2))
        .with_rcode(u4::new(2));
    assert_eq!(s.raw(), 0x1002); // identical to the auto-laid-out DNS State
    assert_eq!(s.opcode(), u5::new(2));
}

#[test]
fn bitfield_seam_exposes_layout_metadata() {
    use bnb::{BitOrder, Bitfield, ByteOrder};

    assert_eq!(<State as Bitfield>::WIDTH, 16);
    assert_eq!(<State as Bitfield>::BYTE_ORDER, ByteOrder::Big);
    assert_eq!(<State as Bitfield>::BIT_ORDER, BitOrder::Msb);
    assert_eq!(<SecurityMode as Bitfield>::BIT_ORDER, BitOrder::Lsb);
    assert_eq!(<Capabilities as Bitfield>::BYTE_ORDER, ByteOrder::Little);

    // to_raw / from_raw are the codec seam a future stream codec builds on.
    let s = State::from_raw(0x1002);
    assert_eq!(Bitfield::to_raw(s), 0x1002u16);
    assert_eq!(<State as Bitfield>::from_raw(0x1002), s);
}

#[test]
fn nested_bitfield_implements_bits_for_composition() {
    // A bitfield nests because it implements `Bits` at its declared width.
    assert_eq!(<OpCode as bnb::Bits>::BITS, 5);
    assert_eq!(<Flags as bnb::Bits>::BITS, 7);
    let op = OpCode::new().with_response(true).with_op(Op::Update);
    // response is the high bit of the 5-bit field (bit 4); op=5 in the low 4.
    assert_eq!(op.raw(), 0b1_0101);
    assert_eq!(bnb::Bits::into_bits(op), 0b1_0101);
}
