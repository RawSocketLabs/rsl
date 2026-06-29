//! **wire_map** — a logical type that serializes via a separate *wire* type, using the
//! conversion-trait form `#[bin(wire = W)]`.
//!
//! `Rgb` is the friendly logical type; `WireColor` is its byte layout on the wire. The
//! transitions live in plain `impl From` blocks — a clean, named home, reusable anywhere via
//! `.into()`, not just at the codec boundary. A one-line `impl FixedBitLen` lets the (fixed-size)
//! mapped type nest as a plain field inside a larger message.
//!
//! Run with: `cargo run -p bitsandbytes --example wire_map`

use bnb::bin;

/// The wire form: three bytes, exactly as they sit on the wire.
#[bin(big)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct WireColor {
    r: u8,
    g: u8,
    b: u8,
}

/// The logical form: a packed 24-bit color. `#[bin(wire = WireColor)]` makes it serialize
/// *through* `WireColor` — so it needs `From<WireColor>` (decode) and `From<&Rgb>` (encode).
#[bin(wire = WireColor)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rgb(u32);

impl From<WireColor> for Rgb {
    fn from(w: WireColor) -> Self {
        Rgb((w.r as u32) << 16 | (w.g as u32) << 8 | w.b as u32)
    }
}
impl From<&Rgb> for WireColor {
    fn from(c: &Rgb) -> Self {
        WireColor {
            r: (c.0 >> 16) as u8,
            g: (c.0 >> 8) as u8,
            b: c.0 as u8,
        }
    }
}
// `WireColor` is fixed-size, so this one line lets `Rgb` nest as a plain field.
impl bnb::FixedBitLen for Rgb {
    const BIT_LEN: u32 = <WireColor as bnb::FixedBitLen>::BIT_LEN;
}

/// A pixel: a mapped `Rgb` nested as a plain field, plus an alpha byte.
#[bin(big)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct Pixel {
    color: Rgb,
    alpha: u8,
}

fn main() {
    // The mapped type encodes/decodes like any `#[bin]` message — the mapping is invisible here.
    let c = Rgb(0x11_22_33);
    assert_eq!(c.to_bytes().unwrap(), [0x11, 0x22, 0x33]);
    assert_eq!(Rgb::decode_exact(&[0x11, 0x22, 0x33]).unwrap(), c);
    println!("Rgb(0x{:06X}) <-> {:02X?}", c.0, c.to_bytes().unwrap());

    // The `From` impls aren't trapped at the codec — they're reusable directly in your program:
    let w: WireColor = (&c).into();
    assert_eq!(
        w,
        WireColor {
            r: 0x11,
            g: 0x22,
            b: 0x33
        }
    );
    let back: Rgb = w.into();
    assert_eq!(back, c);
    println!("the transitions are plain From impls, usable in-program via .into()");

    // And it nests as a plain field thanks to the one-line FixedBitLen:
    let px = Pixel {
        color: Rgb(0x01_02_03),
        alpha: 0xFF,
    };
    assert_eq!(px.to_bytes().unwrap(), [0x01, 0x02, 0x03, 0xFF]);
    assert_eq!(Pixel::decode_exact(&[0x01, 0x02, 0x03, 0xFF]).unwrap(), px);
    println!(
        "Pixel nests the mapped Rgb: {:02X?}",
        px.to_bytes().unwrap()
    );

    println!("all checks passed");
}
