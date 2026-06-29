//! **bitbuf_bounded** — a fixed-capacity `BitBuf` that never reallocates on its own.
//!
//! `BitBuf::bounded(cap)` allocates once. `try_push` refuses bytes that won't fit (returning a
//! `CapacityError`) instead of growing, reclaiming consumed bytes **in place** first; `grow` is
//! the only call that allocates again. Driven as a push/pull stream, a tiny bounded buffer frames
//! an unbounded number of messages while reusing the same allocation — the real-time / `no_std`
//! case where you want a guaranteed-fixed footprint.
//!
//! Run with: `cargo run -p bitsandbytes --example bitbuf_bounded`

use bnb::{BitBuf, bin};

#[bin(big)]
#[derive(Debug, PartialEq, Eq)]
struct Tick {
    seq: u16,
} // 2 bytes each

fn main() {
    // A buffer with room for exactly two messages, allocated once.
    let mut bb = BitBuf::bounded(4);
    assert_eq!(bb.capacity(), Some(4));

    // `try_push` fills it; a third message won't fit until we drain one.
    bb.try_push(&Tick { seq: 1 }.to_bytes().unwrap()).unwrap();
    bb.try_push(&Tick { seq: 2 }.to_bytes().unwrap()).unwrap();
    let full = bb.try_push(&Tick { seq: 3 }.to_bytes().unwrap());
    assert!(full.is_err());
    println!("buffer full: {}", full.unwrap_err()); // CapacityError's Display

    // Pull one → the consumed bytes are reclaimed in place, making room again (no realloc):
    assert_eq!(bb.pull::<Tick>().unwrap(), Some(Tick { seq: 1 }));
    bb.try_push(&Tick { seq: 3 }.to_bytes().unwrap()).unwrap();
    assert_eq!(bb.pull::<Tick>().unwrap(), Some(Tick { seq: 2 }));
    assert_eq!(bb.pull::<Tick>().unwrap(), Some(Tick { seq: 3 }));

    // `grow()` is the ONE call that allocates again — raising the cap explicitly.
    bb.grow(2);
    assert_eq!(bb.capacity(), Some(6));
    println!("grew the cap to {} bytes", bb.capacity().unwrap());

    // Driven as a stream, a tiny bounded buffer frames any number of messages, alloc-once:
    let mut stream = BitBuf::bounded(2);
    for seq in 0..1000u16 {
        stream.try_push(&seq.to_be_bytes()).unwrap(); // always fits — the prior msg was reclaimed
        assert_eq!(stream.pull::<Tick>().unwrap(), Some(Tick { seq }));
    }
    assert!(stream.is_empty());
    println!("framed 1000 messages through a 2-byte bounded buffer (one allocation)");

    println!("all checks passed");
}
