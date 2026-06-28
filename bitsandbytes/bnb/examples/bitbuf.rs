//! **bitbuf** — `BitBuf` push/pull framing, and how it compares to the other ways to decode a
//! buffer you already hold.
//!
//! `BitBuf` owns a growable buffer you **push** bytes into and **pull** whole messages out of.
//! Its real niche is bytes that arrive *incrementally* from something that isn't a `Read` — a
//! channel, an FFI callback, an async poll. For a buffer you already hold **in full**, it is not
//! the simplest tool: `decode_all` / `BitReader` are zero-copy, while `BitBuf` *copies* the bytes
//! into its own `Vec` (and `BufSource` reads them into a retain buffer). This shows all the paths
//! on the same data, so the trade-off is concrete.
//!
//! Run with: `cargo run -p bitsandbytes --example bitbuf`

use bnb::{BitBuf, BitReader, BufSource, bin};

#[bin(big)]
#[derive(Debug, PartialEq, Eq, Clone)]
struct Reading {
    sensor: u8,
    value: u16,
} // 3 bytes each

fn main() {
    let readings = [
        Reading {
            sensor: 1,
            value: 100,
        },
        Reading {
            sensor: 2,
            value: 200,
        },
        Reading {
            sensor: 3,
            value: 300,
        },
    ];
    // an owned buffer of three back-to-back messages (9 bytes)
    let buf: Vec<u8> = readings
        .iter()
        .flat_map(|r| r.to_bytes().unwrap())
        .collect();

    // ── (A) you already hold the whole buffer → simplest, ZERO-COPY ─────────────────────────
    // `decode_all` walks it; `BitReader` is the cursor form. Neither copies the bytes.
    assert_eq!(Reading::decode_all(&buf).unwrap(), readings);
    let mut cur = BitReader::new(&buf); // the zero-copy "wrap an array to call decode" tool
    assert_eq!(Reading::decode(&mut cur).unwrap(), readings[0]); // ...advances the cursor

    // ── (B) BufSource → PULL from a `Read`. A `&[u8]` *is* a `Read`, so it works on an array, ─
    //     but it's built for a real reader (socket/file); over an array it just copies into its
    //     retain buffer. Reach for it when your input is a `Read` that must also seek.
    let mut src = BufSource::new(&buf[..]);
    for want in &readings {
        assert_eq!(&Reading::decode(&mut src).unwrap(), want);
    }

    // ── (C) BitBuf → you PUSH bytes in. This is the niche the others can't cover: chunks that
    //     arrive over time and don't line up with message boundaries. `pull` returns `None`
    //     until a whole message is buffered, reclaiming consumed bytes as it goes.
    let mut bb = BitBuf::new();
    let mut out = Vec::new();
    for chunk in [&buf[0..2], &buf[2..7], &buf[7..9]] {
        bb.push(chunk); // feed whatever just arrived
        while let Some(r) = bb.pull::<Reading>().unwrap() {
            out.push(r); // take every message that's now complete
        }
    }
    assert_eq!(out, readings);
    // ...and unlike BufSource, you can keep pushing more after you've started pulling.

    println!(
        "all three paths decoded the same {} readings:",
        readings.len()
    );
    println!("  (A) decode_all / BitReader : zero-copy — a buffer you already hold (use this)");
    println!("  (B) BufSource              : pull from a Read (socket/file that must seek)");
    println!("  (C) BitBuf                 : push chunks as they arrive, pull complete messages");
}
