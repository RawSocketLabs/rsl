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

// A reserved field is stored; a `temp` field is not.
#[bin]
struct TempReserved {
    #[reserved]
    #[br(temp)]
    #[bw(calc = 0)]
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

// The canonical encoder would write the reserved spec value and drop the `calc`.
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

fn main() {}
