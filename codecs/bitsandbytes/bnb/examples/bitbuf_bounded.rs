//! **`bitbuf_bounded`** — a fixed-capacity `BitBuf` that never reallocates on its own.
//!
//! `BitBuf::bounded(cap)` allocates once. `push` refuses bytes that won't fit (returning a
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

#[allow(clippy::print_stdout)] // This CLI demo intentionally prints its observable results.
fn main() {
    if std::env::args().any(|arg| arg == "--profile") {
        profile_allocations();
        return;
    }
    // A buffer with room for exactly two messages, allocated once.
    let mut bb = BitBuf::bounded(4);
    assert_eq!(bb.capacity(), Some(4));

    // `push` fills it; a third message won't fit until we drain one.
    bb.push(&Tick { seq: 1 }.to_bytes().unwrap()).unwrap();
    bb.push(&Tick { seq: 2 }.to_bytes().unwrap()).unwrap();
    let full = bb.push(&Tick { seq: 3 }.to_bytes().unwrap());
    assert!(full.is_err());
    println!("buffer full: {}", full.unwrap_err()); // CapacityError's Display

    // Pull one → the consumed bytes are reclaimed in place, making room again (no realloc):
    assert_eq!(bb.pull::<Tick>().unwrap(), Some(Tick { seq: 1 }));
    bb.push(&Tick { seq: 3 }.to_bytes().unwrap()).unwrap();
    assert_eq!(bb.pull::<Tick>().unwrap(), Some(Tick { seq: 2 }));
    assert_eq!(bb.pull::<Tick>().unwrap(), Some(Tick { seq: 3 }));

    // `grow()` is the ONE call that allocates again — raising the cap explicitly.
    bb.grow(2);
    assert_eq!(bb.capacity(), Some(6));
    println!("grew the cap to {} bytes", bb.capacity().unwrap());

    // Driven as a stream, a tiny bounded buffer frames any number of messages, alloc-once:
    let mut stream = BitBuf::bounded(2);
    for seq in 0..1000u16 {
        stream.push(&seq.to_be_bytes()).unwrap(); // always fits — the prior msg was reclaimed
        assert_eq!(stream.pull::<Tick>().unwrap(), Some(Tick { seq }));
    }
    assert!(stream.is_empty());
    assert_eq!(stream.capacity(), Some(2)); // the cap never grew — genuinely one allocation
    println!("framed 1000 messages through a 2-byte bounded buffer (one allocation)");

    println!("all checks passed");
}

// Separate from Criterion timing: scripts/profile-allocations.gdb counts system allocator
// calls between these markers without adding unsafe allocator hooks to the crate.
#[inline(never)]
fn allocation_start(phase: usize) {
    std::hint::black_box(phase);
}
#[inline(never)]
fn allocation_end(phase: usize) {
    std::hint::black_box(phase);
}

#[bin(big)]
#[derive(Debug)]
struct Blob {
    #[brw(count_prefix = u16)]
    bytes: Vec<u8>,
}

fn profile_allocations() {
    let mut steady = BitBuf::bounded(2);
    allocation_start(1);
    for seq in 0..1000u16 {
        steady.push(&seq.to_be_bytes()).unwrap();
        assert_eq!(steady.try_pull::<u16>().unwrap(), seq);
    }
    allocation_end(1);

    let mut grown = BitBuf::bounded(32);
    grown.push(&[0]).unwrap();
    grown.grow(32);
    allocation_start(2);
    grown.push(&[1; 63]).unwrap();
    allocation_end(2);

    let mut original = BitBuf::bounded(64);
    original.push(&[0]).unwrap();
    allocation_start(8);
    let mut cloned = original.clone();
    allocation_end(8);
    allocation_start(3);
    cloned.push(&[1; 63]).unwrap();
    allocation_end(3);

    let mut input = BitBuf::bounded(4098);
    input.push(&[0x10, 0]).unwrap();
    allocation_start(4);
    for _ in 0..4095 {
        input.push(&[0x5a]).unwrap();
        assert!(input.try_pull::<Blob>().unwrap_err().is_incomplete());
    }
    allocation_end(4);
    allocation_start(5);
    input.push(&[0x5a]).unwrap();
    let blob = input.try_pull::<Blob>().unwrap();
    assert_eq!(blob.bytes.len(), 4096);
    std::hint::black_box(blob);
    allocation_end(5);

    let mut unbounded = BitBuf::with_capacity(1_000_000);
    unbounded.push(&[1, 2, 3]).unwrap();
    allocation_start(6);
    std::hint::black_box(unbounded.clone());
    allocation_end(6);

    let mut compact = BitBuf::bounded(4);
    compact.push(&[1, 2, 3, 4]).unwrap();
    assert_eq!(compact.try_pull::<u16>().unwrap(), 0x0102);
    allocation_start(7);
    compact.push(&[5, 6]).unwrap();
    allocation_end(7);
    assert_eq!(compact.try_pull::<u32>().unwrap(), 0x0304_0506);

    let mut spare = BitBuf::bounded(65);
    spare.push(&[0x5a; 64]).unwrap();
    assert_eq!(spare.try_pull::<u64>().unwrap(), 0x5a5a_5a5a_5a5a_5a5a);
    allocation_start(9);
    spare.push(&[0xa5]).unwrap();
    allocation_end(9);
    assert_eq!(spare.bit_len(), 57 * 8);

    #[cfg(feature = "net")]
    profile_message_reads();
}

#[cfg(feature = "net")]
fn profile_message_reads() {
    struct OneByte<'a>(&'a [u8]);
    impl std::io::Read for OneByte<'_> {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            let limit = out.len().min(1);
            self.0.read(&mut out[..limit])
        }
    }
    let mut stream = bnb::MessageStream::bounded(std::io::Cursor::new([0x12, 0x34]), 2);
    allocation_start(10);
    for _ in 0..1000 {
        stream.get_mut().set_position(0);
        assert_eq!(stream.read_message::<u16>().unwrap(), 0x1234);
    }
    allocation_end(10);

    let mut wire = [0x5a; 4098];
    wire[..2].copy_from_slice(&4096_u16.to_be_bytes());
    let mut buffer = BitBuf::bounded(wire.len());
    let mut reader = OneByte(&wire);
    let mut scratch = [0; 256];
    allocation_start(11);
    let blob: Blob = bnb::net::read_message(&mut reader, &mut buffer, &mut scratch).unwrap();
    assert_eq!(blob.bytes.len(), 4096);
    std::hint::black_box(blob);
    allocation_end(11);

    #[cfg(feature = "tokio-io")]
    {
        use std::{
            future::Future,
            task::{Context, Poll, Waker},
        };
        let mut reader = &wire[..];
        allocation_start(12);
        let future =
            bnb::net::read_message_async::<Blob, _>(&mut reader, &mut buffer, &mut scratch);
        let mut future = std::pin::pin!(future);
        let Poll::Ready(result) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        else {
            panic!("in-memory transport is always ready");
        };
        assert_eq!(result.unwrap().bytes.len(), 4096);
        allocation_end(12);
    }
}
