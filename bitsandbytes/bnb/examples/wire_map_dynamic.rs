//! **wire_map_dynamic** — the wire-mapping forms that the conversion-trait approach unlocks:
//! a **variable-length** wire form, the inline **closure** form, and the fallible **`try_wire`**
//! form.
//!
//! Because a mapped type doesn't auto-implement `FixedBitLen`, its wire form may be
//! variable-length — here a `String` maps to/from a length-prefixed `WireText`. The closure form
//! (`map`/`bw_map`) is the quick inline alternative for a small fixed mapping, and `try_wire`
//! (driven by `TryFrom`) rejects bad input at decode.
//!
//! Run with: `cargo run -p bitsandbytes --example wire_map_dynamic`

use bnb::bin;

// --- (A) variable-length wire form, via the conversion-trait form ----------------

/// A length-prefixed wire string: `n` then `n` bytes (variable-length — no `FixedBitLen`).
#[bin(big)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct WireText {
    n: u8,
    #[br(count = n)]
    data: Vec<u8>,
}

/// The logical form: a real `String`, mapped to/from `WireText`.
#[bin(wire = WireText)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct Greeting(String);
impl From<WireText> for Greeting {
    fn from(w: WireText) -> Self {
        Greeting(String::from_utf8_lossy(&w.data).into_owned())
    }
}
impl From<&Greeting> for WireText {
    fn from(g: &Greeting) -> Self {
        WireText {
            n: g.0.len() as u8,
            data: g.0.as_bytes().to_vec(),
        }
    }
}

// --- (B) inline closure form, for a quick fixed mapping --------------------------

/// A temperature stored on the wire as one biased byte (celsius + 40).
#[bin(
    map = |raw: u8| Celsius(raw as i16 - 40),
    bw_map = |c: &Celsius| (c.0 + 40) as u8
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Celsius(i16);

// --- (C) fallible conversion-trait form, via try_wire ---------------------------

#[bin(big)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct WirePercent {
    raw: u8,
}

/// A percentage in `0..=100`; a wire byte over 100 is rejected at decode.
#[bin(try_wire = WirePercent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Percent(u8);
impl TryFrom<WirePercent> for Percent {
    type Error = &'static str;
    fn try_from(w: WirePercent) -> Result<Self, Self::Error> {
        if w.raw <= 100 {
            Ok(Percent(w.raw))
        } else {
            Err("percent over 100")
        }
    }
}
impl From<&Percent> for WirePercent {
    fn from(p: &Percent) -> Self {
        WirePercent { raw: p.0 }
    }
}

fn main() {
    // (A) variable-length: a String-backed logical type over a length-prefixed wire.
    let g = Greeting("hello".into());
    assert_eq!(g.to_bytes().unwrap(), [5, b'h', b'e', b'l', b'l', b'o']);
    assert_eq!(
        Greeting::decode_exact(&[2, b'h', b'i']).unwrap(),
        Greeting("hi".into())
    );
    // decode_all walks several variable-length messages back to back:
    let many = Greeting::decode_all(&[1, b'a', 3, b'b', b'c', b'd']).unwrap();
    assert_eq!(many, vec![Greeting("a".into()), Greeting("bcd".into())]);
    println!(
        "(A) variable-length: {:?} -> {:02X?}",
        g.0,
        g.to_bytes().unwrap()
    );

    // (B) closure form: the quick inline path for a fixed mapping.
    assert_eq!(Celsius(25).to_bytes().unwrap(), [65]); // 25 + 40
    assert_eq!(Celsius::decode_exact(&[10]).unwrap(), Celsius(-30)); // 10 - 40
    println!(
        "(B) closure-mapped Celsius(25) -> {:?}",
        Celsius(25).to_bytes().unwrap()
    );

    // (C) try_wire: a valid value round-trips; an out-of-range wire byte is a decode error.
    assert_eq!(Percent::decode_exact(&[80]).unwrap(), Percent(80));
    assert_eq!(Percent(80).to_bytes().unwrap(), [80]);
    assert!(Percent::decode_exact(&[200]).is_err());
    println!("(C) try_wire rejects 200 (>100) at decode");

    println!("all checks passed");
}
