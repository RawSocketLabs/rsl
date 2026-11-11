//! Field directives that cannot share a field are rejected at the later directive, instead
//! of one of them being silently dropped (or the read and write sides disagreeing).
use bnb::bin;

// `if` reads the `Option`'s inner type with its own codec: the `map` would be dropped.
#[bin]
struct IfMap {
    flag: u8,
    #[br(if(flag != 0), map = |x: u8| u16::from(x))]
    #[bw(map = |v: &u16| *v as u8)]
    value: Option<u16>,
}

// `calc` writes with the field type's own codec: the `bw(map)` would be dropped.
#[bin]
struct CalcMap {
    #[bw(calc = 1, map = |v: &u8| *v + 1)]
    len: u8,
}

// A `temp` field is not stored, so without a `calc` the reserved value stands in for
// nothing. (With one it is accepted: `tests/bin_reserved.rs`.)
#[bin(read_only)]
struct TempReserved {
    #[reserved]
    #[br(temp)]
    pad: u8,
}

// An ignored field is neither read nor written: the `count` would never run.
#[bin]
struct IgnoreCount {
    n: u8,
    #[brw(ignore)]
    #[br(count = n)]
    cache: Vec<u8>,
}

// A mapped field is read by its closure's wire type: the `count` would be dropped.
#[bin]
struct CountMap {
    n: u8,
    #[br(count = n, map = |x: [u8; 2]| x.to_vec())]
    #[bw(map = |v: &Vec<u8>| [v[0], v[1]])]
    items: Vec<u8>,
}

// A `restore_position` field is a peek and is never written: the `write_with` would be
// dropped.
#[bin]
struct PeekWrite {
    #[br(restore_position)]
    #[bw(write_with = bnb::codecs::leb128::write)]
    peeked: u64,
    body: u8,
}

// A stored field would get two canonical values: the reserved one in
// `to_canonical_bytes()`, the `calc` in `to_canonical()`.
#[bin]
struct ReservedCalc {
    #[reserved]
    #[bw(calc = 7)]
    pad: u8,
}

// `br(calc)` reads nothing, so there is nothing to map (the pre-existing rule, now spanned
// at the directive).
#[bin]
struct CalcRead {
    raw: u8,
    #[br(calc = raw, map = |x: u8| x)]
    derived: u8,
}

// An ignored field is defaulted on decode: the `calc` would never run (the other
// pre-existing `br(calc)` rule).
#[bin]
struct CalcIgnore {
    raw: u8,
    #[br(calc = raw & 1)]
    #[brw(ignore)]
    derived: u8,
}

// A `temp` peek is neither stored nor written: its `calc` would never run. (A stored peek
// may carry one; the canonical helpers recompute it.)
#[bin]
struct TempPeekCalc {
    #[br(temp, restore_position)]
    #[bw(calc = self.body)]
    tag: u8,
    body: u8,
}

// The canonical encoder would write the reserved value as a plain `u16`, bypassing the
// `bw(map)` that makes the field one byte on the wire.
#[bin]
struct ReservedMap {
    #[reserved]
    #[br(map = |x: u8| u16::from(x))]
    #[bw(map = |v: &u16| *v as u8)]
    pad: u16,
}

#[bin(ctx(wide: u8))]
struct Child {
    #[br(count = wide)]
    data: Vec<u8>,
}

// `if` reads the inner type with its own codec: the context would never be passed.
#[bin]
struct IfCtx {
    flag: u8,
    wide: u8,
    #[br(if(flag != 0), ctx { wide })]
    child: Option<Child>,
}

fn main() {}
