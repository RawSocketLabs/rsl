//! **conditional** — `#[bin]` `if`: fields that appear on the wire only when an earlier field
//! says so (decoded as `Option<T>`, present ⇒ written), plus `map` to convert a wire integer
//! into a domain newtype. The "compact header + optional extensions" orchestration.
//!
//! Run with: `cargo run -p bitsandbytes --example conditional`

use bnb::bin;

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

#[bin(big)]
#[derive(Debug, PartialEq, Clone)]
struct Reading {
    sensor_id: u8,
    flags: u8, // bit0 = has_auth, bit1 = has_battery
    #[br(map = |raw: u16| Celsius(raw as i16 as f32 / 100.0))]
    #[bw(map = |c: &Celsius| ((c.0 * 100.0) as i16) as u16)]
    temp: Celsius,
    #[br(if(flags & 0x01 != 0))]
    auth_token: Option<u32>, // an optional scalar
    #[br(if(flags & 0x02 != 0))]
    battery: Option<Battery>, // an optional nested message
}

fn main() {
    // A full reading: auth token + battery present.
    let full = Reading {
        sensor_id: 7,
        flags: 0b11, // has_auth | has_battery
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
    let minimal = Reading {
        sensor_id: 7,
        flags: 0,
        temp: Celsius(-3.0), // sub-zero round-trips via the i16 reinterpretation
        auth_token: None,
        battery: None,
    };
    let bytes = minimal.to_bytes().unwrap();
    println!("minimal reading: {:>2} bytes  {bytes:02x?}", bytes.len());
    assert_eq!(Reading::decode_exact(&bytes).unwrap(), minimal);

    println!("all checks passed");
}
