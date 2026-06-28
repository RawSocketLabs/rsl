//! **wav** — little-endian byte order (`#[bin(little)]`): a WAV/RIFF `fmt ` chunk.
//!
//! Most examples here are big-endian (network order). RIFF containers — WAV, AVI — and most file
//! formats are little-endian: a multi-byte integer is stored low byte first. This decodes a WAV
//! `fmt ` subchunk; flip `little` to `big` and every multi-byte field byte-swaps. (`can_signals`
//! covers the lower-level axis — LSB-first *bit* packing.)
//!
//! Run with: `cargo run -p bitsandbytes --example wav`

use bnb::bin;

/// A WAV `fmt ` subchunk (PCM) — every field little-endian.
#[bin(little)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct WavFmt {
    #[try_str] // render the 4-byte tag as "fmt " rather than raw decimals
    id: [u8; 4],
    size: u32,         // 16 for PCM
    audio_format: u16, // 1 = PCM
    channels: u16,
    sample_rate: u32, // Hz
    byte_rate: u32,   // sample_rate * block_align
    block_align: u16, // channels * bits_per_sample / 8
    bits_per_sample: u16,
}

fn main() {
    let fmt = WavFmt {
        id: *b"fmt ",
        size: 16,
        audio_format: 1,
        channels: 2,
        sample_rate: 44_100,
        byte_rate: 176_400,
        block_align: 4,
        bits_per_sample: 16,
    };
    let bytes = fmt.to_bytes().unwrap();
    println!("encoded: {} bytes  {bytes:02x?}", bytes.len());
    // little-endian: sample_rate 44_100 = 0x0000_AC44, stored low byte first at offset 12.
    assert_eq!(&bytes[12..16], &[0x44, 0xAC, 0x00, 0x00]);
    assert_eq!(WavFmt::decode_exact(&bytes).unwrap(), fmt);
    println!("{fmt:#?}");
    println!("all checks passed");
}
