//! **build_syn** — construct a SYN segment, encode it, and confirm it round-trips.
//!
//! Run with: `cargo run -p tcp --example build_syn`
#![allow(clippy::print_stdout)]

use tcp::{Control, TcpHeader};

fn main() {
    let syn = TcpHeader::segment(
        40000,                         // src port
        80,                            // dst port
        0x1000_0000,                   // seq
        0,                             // ack (unused without ACK)
        Control::new().with_syn(true), // flags
        65535,                         // window
        vec![0x02, 0x04, 0x05, 0xb4],  // one MSS=1460 option
    );

    let wire = syn.to_bytes().unwrap();
    println!("SYN to port 80 ({} bytes): {:02x?}", wire.len(), wire);
    println!(
        "data_offset computed = {} words ({} bytes)",
        u8::from(syn.control.data_offset()),
        syn.header_len()
    );

    // Round-trips.
    assert_eq!(TcpHeader::decode_exact(&wire).unwrap(), syn);
    println!("round-trips ✓");
}
