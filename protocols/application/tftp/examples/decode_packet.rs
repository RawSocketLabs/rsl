//! Low-level demo: decode a TFTP datagram into a typed packet and inspect it,
//! using only the [`wire`](tftp::wire) layer (no sockets).
//!
//! Run with: `cargo run -p tftp --example decode_packet`

use tftp::wire::{Packet, Request, TransferMode};

fn main() -> Result<(), tftp::error::TftpError> {
    // Build an RRQ and look at its bytes.
    let rrq = Request::read("kernel.img", TransferMode::Octet);
    let bytes = rrq.encode();
    println!("RRQ on the wire: {bytes:02x?}");

    // Decode an arbitrary datagram back into a typed packet.
    match Packet::decode(&bytes)? {
        Packet::Request(req) => {
            println!(
                "decoded {:?} for {:?} in {} mode",
                req.kind,
                req.filename_lossy(),
                req.mode
            );
        }
        other => println!("decoded a {:?} packet", other.opcode()),
    }

    // The codec is liberal: an unknown opcode is a decode error, not a panic.
    let bogus = [0x00, 0x63, 0x00, 0x00];
    println!(
        "decoding an unknown opcode yields: {:?}",
        Packet::decode(&bogus)
    );
    Ok(())
}
