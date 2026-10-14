//! Stateful buffer/input accounting, independently modeled as length-prefixed bytes.
#![no_main]
use bnb::{BitBuf, MessageStream, Source, bin};
use libfuzzer_sys::fuzz_target;
use std::io::Read;
use std::{
    future::Future,
    io,
    pin::Pin,
    task::{Context, Poll, Waker},
};
use tokio::io::{AsyncRead, ReadBuf};

struct Input<'a> {
    bytes: &'a [u8],
    chunk: usize,
    pending: bool,
}

impl Read for Input<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let limit = out.len().min(self.chunk);
        self.bytes.read(&mut out[..limit])
    }
}

impl AsyncRead for Input<'_> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        out: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.pending = !self.pending;
        if self.pending {
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            let count = self.read(out.initialize_unfilled())?;
            out.advance(count);
            Poll::Ready(Ok(()))
        }
    }
}

// Exercise the actual refill paths, including cancellation after every completed
// async fragment, capacity rejection, coalesced records and lossless raw handoff.
fn message_reads(data: &[u8]) {
    let Some((&config, bytes)) = data.split_first() else {
        return;
    };
    let bytes = &bytes[..bytes.len().min(512)];
    let cap = usize::from(config % 64) + 1;
    let chunk = usize::from(config % 17) + 1;
    for asynchronous in [false, true] {
        let mut input = Input {
            bytes,
            chunk,
            pending: false,
        };
        let mut buffer = BitBuf::bounded(cap);
        let mut scratch = [0; 32];
        let scratch = &mut scratch[..usize::from(config % 32) + 1];
        let mut consumed = 0;
        loop {
            let result = if asynchronous {
                let mut polls = 0;
                loop {
                    polls += 1;
                    assert!(
                        polls <= bytes.len() * 2 + 4,
                        "reader must make bounded progress"
                    );
                    let future =
                        bnb::net::read_message_async::<Frame, _>(&mut input, &mut buffer, scratch);
                    let mut future = std::pin::pin!(future);
                    // Drop on Pending: retained bytes, not the future, own progress.
                    if let Poll::Ready(result) = future
                        .as_mut()
                        .poll(&mut Context::from_waker(Waker::noop()))
                    {
                        break result;
                    }
                }
            } else {
                bnb::net::read_message::<Frame, _>(&mut input, &mut buffer, scratch)
            };
            let complete = bytes
                .get(consumed)
                .map(|&length| usize::from(length) + 1)
                .filter(|&length| length <= cap && length <= bytes.len() - consumed);
            if let Some(length) = complete {
                assert_eq!(
                    result.unwrap().bytes,
                    bytes[consumed + 1..consumed + length]
                );
                consumed += length;
            } else {
                assert!(result.is_err());
                let mut handoff = MessageStream::from_parts(input, buffer);
                let mut tail = Vec::new();
                handoff.read_to_end(&mut tail).unwrap();
                assert_eq!(tail, bytes[consumed..]);
                break;
            }
        }
    }
}

#[bin(big)]
#[derive(Debug)]
struct Frame {
    #[brw(count_prefix = u8)]
    bytes: Vec<u8>,
}

#[bin(big)]
#[derive(Debug)]
enum Magic {
    #[bin(magic = b"AB")]
    Long,
    #[bin(magic = b"A")]
    Short,
    Raw(u8),
}

fuzz_target!(|data: &[u8]| {
    message_reads(data);
    let mut buffer = BitBuf::bounded(64);
    let mut model = Vec::new();
    // Fixed-size operations keep the harness's own resource use bounded as well.
    for operation in data.chunks(9) {
        let (&code, bytes) = operation.split_first().unwrap();
        match code % 7 {
            0 => {
                let position = buffer.bit_pos();
                let fits = model.len() + bytes.len() <= buffer.capacity().unwrap();
                assert_eq!(buffer.push(bytes).is_ok(), fits);
                if fits {
                    model.extend_from_slice(bytes);
                } else {
                    assert_eq!(buffer.bit_pos(), position);
                }
            }
            1 | 2 => {
                let eof = code % 7 == 2;
                let complete = model
                    .first()
                    .copied()
                    .filter(|&n| model.len() > usize::from(n));
                let position = buffer.bit_pos();
                let decoded = if eof {
                    buffer.pull_eof::<Frame>()
                } else {
                    buffer.try_pull::<Frame>().map(Some)
                };
                match complete {
                    Some(n) => {
                        let n = usize::from(n);
                        assert_eq!(decoded.unwrap().unwrap().bytes, model[1..=n]);
                        model.drain(..=n);
                    }
                    None if eof && model.is_empty() => assert!(decoded.unwrap().is_none()),
                    None => {
                        let error = decoded.unwrap_err();
                        assert_eq!(error.is_incomplete(), !eof);
                        assert_eq!(buffer.bit_pos(), position);
                    }
                }
            }
            3 => buffer.compact(),
            4 if buffer.capacity().unwrap() < 128 => buffer.grow(8),
            5 => {
                let stream = MessageStream::from_parts(&[][..], buffer);
                let (inner, retained) = match stream.try_into_inner() {
                    Ok(inner) => {
                        assert!(model.is_empty());
                        (inner, BitBuf::bounded(64))
                    }
                    Err(stream) => stream.into_parts(),
                };
                let mut stream = MessageStream::from_parts(inner, retained);
                let mut raw = Vec::new();
                stream.read_to_end(&mut raw).unwrap();
                assert_eq!(raw, model);
                let (_, retained) = stream.into_parts();
                buffer = retained;
                model.clear();
            }
            6 => {
                buffer.clear();
                model.clear();
            }
            _ => {}
        }
        assert_eq!(buffer.bit_len(), model.len() * 8);
        // Also exercise prefix discrimination on arbitrarily fragmented, bounded input.
        let mut magic = BitBuf::bounded(8);
        for &byte in bytes {
            magic.push(&[byte]).unwrap();
            loop {
                let before = magic.bit_len();
                match magic.try_pull::<Magic>() {
                    Ok(_) => assert!(magic.bit_len() < before),
                    Err(error) => {
                        assert!(error.is_incomplete());
                        break;
                    }
                }
            }
        }
        while magic.pull_eof::<Magic>().unwrap().is_some() {}
        assert!(magic.is_empty());
    }
});
