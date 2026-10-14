//! **conditional** — `#[bin]` `if`: fields that appear on the wire only when an earlier field
//! says so (decoded as `Option<T>`, present ⇒ written), plus `map` to convert a wire integer
//! into a domain newtype. The presence flags are a real `#[bitflags]` set — named accessors,
//! not magic masks. The "compact header + optional extensions" orchestration.
//!
//! Run with: `cargo run -p bitsandbytes --example conditional`

use bnb::{bin, bitflags};

/// Centidegrees on the wire (a `u16` reinterpreted as `i16` for sub-zero temps), a typed
/// `Celsius` in memory — bridged by `map`.
#[derive(Debug, PartialEq, Clone, Copy)]
struct Celsius(f32);

/// An optional battery extension (a nested message), present only when its flag is set.
#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Battery {
    percent: u8,
    millivolts: u16,
}

/// The presence flags — one wire byte, bits auto-assigned LSB-first (bit0 = `has_auth`,
/// bit1 = `has_battery`), read through named accessors instead of `& 0x01` masks.
#[bitflags(u8)]
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
struct Presence {
    has_auth: bool,
    has_battery: bool,
}

#[bin(big)]
#[derive(Debug, PartialEq, Clone)]
struct Reading {
    sensor_id: u8,

    flags: Presence,

    #[br(map = |raw: u16| Celsius(f32::from(i16::from_ne_bytes(raw.to_ne_bytes())) / 100.0))]
    #[bw(map = encode_temperature)]
    temp: Celsius,

    #[br(if(flags.has_auth()))]
    auth_token: Option<u32>, // an optional scalar

    #[br(if(flags.has_battery()))]
    battery: Option<Battery>, // an optional nested message
}

#[allow(clippy::cast_possible_truncation)] // Quantize hundredths of a degree to signed 16-bit storage.
#[allow(clippy::trivially_copy_pass_by_ref)] // The wire-map callback borrows the field.
fn encode_temperature(c: &Celsius) -> u16 {
    u16::from_ne_bytes(((c.0 * 100.0) as i16).to_ne_bytes())
}

#[allow(clippy::print_stdout)] // This CLI demo intentionally prints its observable results.
fn main() {
    // A full reading: auth token + battery present.
    let full = Reading {
        sensor_id: 7,
        flags: Presence::HAS_AUTH | Presence::HAS_BATTERY,
        temp: Celsius(21.5),
        auth_token: Some(0xDEAD_BEEF),
        battery: Some(Battery {
            percent: 88,
            millivolts: 3700,
        }),
    };
    let bytes = full.to_bytes().unwrap();
    println!("full reading:    {:>2} bytes  {bytes:02x?}", bytes.len());
    assert_eq!(Reading::decode_exact(&bytes).unwrap(), full);
    println!("{full:#?}");

    // A minimal reading: both optionals absent — they consume zero bytes on the wire.
    // (sub-zero temp round-trips via the i16 reinterpretation)
    let minimal = Reading {
        sensor_id: 7,
        flags: Presence::empty(),
        temp: Celsius(-3.0),
        auth_token: None,
        battery: None,
    };
    let bytes = minimal.to_bytes().unwrap();
    println!("minimal reading: {:>2} bytes  {bytes:02x?}", bytes.len());
    assert_eq!(Reading::decode_exact(&bytes).unwrap(), minimal);

    println!("all checks passed");
}
