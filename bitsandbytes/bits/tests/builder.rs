//! `#[derive(BitsBuilder)]`: required-by-default builder semantics, via the
//! `#[bitfield]` intercept and on a plain struct. Codec-only (no binrw needed).

use bits::{bitfield, u4, BitEnum, BitsBuilder, BuilderError};

#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[bit_enum(u4)]
enum RCode {
    #[default]
    NoError,
    FormErr,
    ServFail,
    #[catch_all]
    Other(u4),
}

// `#[bitfield]` is above `#[derive]` so it intercepts BitsBuilder.
#[bitfield(u16, bits = msb)]
#[derive(BitsBuilder, Clone, Copy, Debug, PartialEq, Eq)]
struct State {
    opcode: u4, // required
    #[builder(default)] // optional -> 0
    flags: u8,
    rcode: RCode, // required
}

#[test]
fn unset_required_field_errors() {
    let err = State::builder().build().unwrap_err();
    assert_eq!(err, BuilderError::missing_field("opcode"));
    assert_eq!(err.to_string(), "required field `opcode` was not set");

    // Set opcode but not rcode -> rcode is now the missing one.
    let err = State::builder().opcode(u4::new(1)).build().unwrap_err();
    assert_eq!(err.field(), "rcode");
}

#[test]
fn builds_with_required_set_and_default_applied() {
    let s = State::builder()
        .opcode(u4::new(2))
        .rcode(RCode::ServFail)
        // flags omitted -> #[builder(default)] -> 0
        .build()
        .unwrap();

    assert_eq!(s.opcode(), u4::new(2));
    assert_eq!(s.rcode(), RCode::ServFail);
    assert_eq!(s.flags(), 0);

    // Identical to the infix path; the builder just adds required-field checks.
    assert_eq!(s, State::new().with_opcode(u4::new(2)).with_rcode(RCode::ServFail));
}

// `#[builder(default = expr)]` for a non-Default value.
#[bitfield(u8, bits = msb)]
#[derive(BitsBuilder, Clone, Copy, Debug, PartialEq, Eq)]
struct VersionIhl {
    version: u4,
    #[builder(default = u4::new(5))] // default header length 5
    ihl: u4,
}

#[test]
fn default_expr_is_used_when_unset() {
    let v = VersionIhl::builder().version(u4::new(4)).build().unwrap();
    assert_eq!(v.to_be_bytes(), [0x45]); // version 4, ihl defaulted to 5
    let v = VersionIhl::builder().version(u4::new(6)).ihl(u4::new(7)).build().unwrap();
    assert_eq!(v.to_be_bytes(), [0x67]);
}

// The same derive works on a plain (non-bitfield) struct.
#[derive(BitsBuilder, Debug, PartialEq, Eq)]
struct Config {
    name: String, // required
    #[builder(default = 69)]
    port: u16,
    #[builder(default)]
    verbose: bool,
}

#[test]
fn plain_struct_builder() {
    let err = Config::builder().build().unwrap_err();
    assert_eq!(err.field(), "name");

    let c = Config::builder().name("svc".to_string()).build().unwrap();
    assert_eq!(c, Config { name: "svc".to_string(), port: 69, verbose: false });

    let c = Config::builder()
        .name("svc".to_string())
        .port(8080)
        .verbose(true)
        .build()
        .unwrap();
    assert_eq!(c, Config { name: "svc".to_string(), port: 8080, verbose: true });
}
