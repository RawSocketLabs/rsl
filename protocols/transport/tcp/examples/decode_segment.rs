//! **decode_segment** — decode a TCP header and print its ports, sequence numbers, and the
//! set control flags.
//!
//! Run with: `cargo run -p tcp --example decode_segment`
#![allow(clippy::print_stdout)]

use tcp::TcpHeader;

fn main() {
    // A SYN-ACK: data_offset=6 (0x6012 = SYN+ACK), one MSS option.
    let wire = [
        0x00, 0x50, 0x9c, 0x40, 0xaa, 0xbb, 0xcc, 0xdd, 0x12, 0x34, 0x56, 0x79, //
        0x60, 0x12, // data_offset=6, SYN+ACK
        0x72, 0x10, 0x00, 0x00, 0x00, 0x00, //
        0x02, 0x04, 0x05, 0xb4, // MSS 1460
    ];
    let h = TcpHeader::decode_exact(&wire).unwrap();

    println!("{} -> {}", h.src_port, h.dst_port);
    println!(
        "seq={:#010x} ack={:#010x} window={}",
        h.seq, h.ack, h.window
    );
    print!("flags:");
    for (name, set) in [
        ("SYN", h.is_syn()),
        ("ACK", h.is_ack()),
        ("FIN", h.is_fin()),
        ("RST", h.is_rst()),
        ("PSH", h.control.psh()),
        ("URG", h.control.urg()),
    ] {
        if set {
            print!(" {name}");
        }
    }
    println!();
    println!(
        "header {} bytes, {} option bytes: {:02x?}",
        h.header_len(),
        h.options.len(),
        h.options
    );
    for opt in h.options_parsed() {
        println!("  option: {opt:?}");
    }
}
