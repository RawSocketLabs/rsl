//! **heartbeat** — a device status report combining three directives at once: `#[bitflags]`
//! status bits, a `map` to a typed voltage, and an `if` field gated by a **flag** (a third `if`
//! driver, after `conditional`'s bit-test and `versioned`'s version compare). A fault code rides
//! along only when the `FAULT` flag is set.
//!
//! Run with: `cargo run -p bitsandbytes --example heartbeat`

use bnb::{bin, bitflags};

#[bitflags(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
struct Status {
    online: bool,
    charging: bool,
    fault: bool,
    low_battery: bool,
}

/// Tenths of a volt on the wire (`u16`), a typed `DeciVolts` in memory — bridged by `map`.
#[derive(Debug, PartialEq, Clone, Copy)]
struct DeciVolts(f32);

#[bin(big)]
#[derive(Debug, PartialEq, Clone)]
struct Heartbeat {
    device_id: u16,
    status: Status,
    #[br(map = |raw: u16| DeciVolts(raw as f32 / 10.0))]
    #[bw(map = |v: &DeciVolts| (v.0 * 10.0) as u16)]
    voltage: DeciVolts,
    // Present only when the FAULT flag is set — `if` reads the decoded `status` local.
    #[br(if(status.contains(Status::FAULT)))]
    fault_code: Option<u16>,
}

fn main() {
    // Healthy: no fault flag, so no fault code on the wire.
    let ok = Heartbeat {
        device_id: 0x0042,
        status: Status::ONLINE | Status::CHARGING,
        voltage: DeciVolts(12.5),
        fault_code: None,
    };
    let bytes = ok.to_bytes().unwrap();
    println!("{ok:#?}");
    println!("  -> {} bytes  {bytes:02x?}", bytes.len());
    assert_eq!(Heartbeat::decode_exact(&bytes).unwrap(), ok);

    // Faulted: the FAULT flag pulls in a fault code.
    let faulted = Heartbeat {
        device_id: 0x0043,
        status: Status::ONLINE | Status::FAULT,
        voltage: DeciVolts(9.5),
        fault_code: Some(0x0102),
    };
    let bytes = faulted.to_bytes().unwrap();
    println!("{faulted:#?}");
    println!("  -> {} bytes  {bytes:02x?}", bytes.len());
    assert_eq!(Heartbeat::decode_exact(&bytes).unwrap(), faulted);

    println!("all checks passed");
}
