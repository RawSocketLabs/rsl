//! Async framing via [`tokio_util::codec`] — the `tokio` feature.
//!
//! [`BinCodec<T>`] implements [`Decoder`](tokio_util::codec::Decoder) +
//! [`Encoder`](tokio_util::codec::Encoder) for **any** `#[bin]` message (anything that is
//! [`BitDecode`]/[`BitEncode`]), so wrapping a tokio stream is one line:
//!
//! ```ignore
//! use bnb::BinCodec;
//! use tokio_util::codec::Framed;
//! use futures_util::{SinkExt, StreamExt};
//!
//! let mut conn = Framed::new(tcp_stream, BinCodec::<MyMsg>::new());
//! conn.send(MyMsg::Ping).await?;          // it's a `Sink<MyMsg>`
//! let reply = conn.next().await.unwrap()?; // …and a `Stream<Item = MyMsg>`
//! ```
//!
//! The codec relies on each message being **self-delimiting** (its `#[bin]` structure or a
//! `magic`/length prefix bounds it) — `decode` reads exactly one message and returns `None`
//! when only a partial frame has arrived, so `Framed` reads more and retries.
//!
//! **Byte alignment over `Framed`.** `Framed`'s buffer is byte-granular, so `BinCodec`
//! consumes the final byte's padding and starts each `decode` at a byte boundary, matching
//! the encoder. Padding bits are accepted, not validated. For bit-packed concatenation
//! without per-message padding, use [`BitBuf`](crate::BitBuf) directly.
//!
//! `decode` leaves the entire buffer unchanged on incomplete input or errors. `decode_eof`
//! makes a finite attempt so truncation is an error. Parsing is replayed on each attempt;
//! callbacks must be retry-safe. Use `Framed::into_parts` / `Framed::from_parts` for protocol
//! transitions, transferring **both** read and write buffers; `into_inner` discards them.
//! This convenience codec uses `T::LAYOUT` from `BitEncode`; directional/contextual codecs
//! instead use [`BitBuf::try_pull_with`](crate::BitBuf::try_pull_with) with an explicit layout.
//!
//! The same `BinCodec` drives **datagrams**, too: `tokio_util::udp::UdpFramed::new(udp_socket,
//! BinCodec::<T>::new())` is a `Stream<Item = (T, SocketAddr)>` + `Sink<(T, SocketAddr)>`. So
//! one codec covers async streams (`Framed`, TCP) and async datagrams (`UdpFramed`, UDP) — the
//! async mirror of the sync `MessageStream` / `MessageDatagram` split. See `examples/tokio_udp`.

use crate::{BitDecode, BitEncode, BitWriter};
use bytes::{Buf, BytesMut};
use core::marker::PhantomData;
use std::io;
use tokio_util::codec::{Decoder, Encoder};

/// A [`tokio_util::codec`] codec that frames any bnb `#[bin]` message `T`. Construct with
/// [`BinCodec::new`] and hand it to [`Framed`](tokio_util::codec::Framed).
pub struct BinCodec<T>(PhantomData<T>);

impl<T> BinCodec<T> {
    /// A codec for messages of type `T`.
    #[must_use]
    pub fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T> Default for BinCodec<T> {
    fn default() -> Self {
        Self::new()
    }
}

// `Decoder` also bounds `T: BitEncode` so it can read in the message's declared
// [`LAYOUT`](BitEncode::LAYOUT) — a `#[bin(little)]`/`bits = lsb` message must decode in
// the same order the paired `Encoder` writes it, or the round-trip silently corrupts. (A
// `BinCodec` used over `Framed`/`UdpFramed` always implements both directions anyway.)
impl<T: BitDecode + BitEncode> Decoder for BinCodec<T> {
    type Item = T;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<T>, io::Error> {
        Self::attempt(src, false)
    }

    fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<T>, io::Error> {
        Self::attempt(src, true)
    }
}

impl<T: BitDecode + BitEncode> BinCodec<T> {
    fn attempt(src: &mut BytesMut, eof: bool) -> Result<Option<T>, io::Error> {
        if src.is_empty() {
            return Ok(None);
        }
        match crate::bitstream::decode_attempt(&src[..], 0, T::LAYOUT, (), eof) {
            Ok((item, end)) => {
                src.advance(end.div_ceil(8));
                Ok(Some(item))
            }
            // Only a partial frame is buffered — ask `Framed` to read more (don't consume).
            Err(e) if e.is_incomplete() => Ok(None),
            // A genuine framing error.
            Err(e) => Err(io::Error::new(io::ErrorKind::InvalidData, e)),
        }
    }
}

impl<T: BitEncode> Encoder<T> for BinCodec<T> {
    type Error = io::Error;

    fn encode(&mut self, item: T, dst: &mut BytesMut) -> Result<(), io::Error> {
        let mut w = BitWriter::with_layout(<T as BitEncode>::LAYOUT);
        item.bit_encode(&mut w)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        dst.extend_from_slice(&w.into_bytes());
        Ok(())
    }
}

#[cfg(test)]
mod component {
    //! Component tests: the `BinCodec` Decoder/Encoder framing logic over a `BytesMut` (one
    //! message per `decode`, partial-frame `None`, exact consumption, error mapping).
    use super::BinCodec;
    use bnb::bin;
    use bytes::BytesMut;
    use tokio_util::codec::{Decoder, Encoder};

    #[test]
    fn padded_subbyte_frames_consume_the_whole_final_byte() {
        let mut short = BinCodec::<bnb::u4>::new();
        let mut long = BinCodec::<bnb::u12>::new();
        for short_first in [false, true] {
            let mut wire = BytesMut::new();
            if short_first {
                short.encode(bnb::u4::new(0xa), &mut wire).unwrap();
            }
            long.encode(bnb::u12::new(0xbcd), &mut wire).unwrap();
            if !short_first {
                short.encode(bnb::u4::new(0xa), &mut wire).unwrap();
            }
            wire.extend_from_slice(b"raw");
            if short_first {
                assert_eq!(short.decode(&mut wire).unwrap().unwrap().value(), 0xa);
            }
            assert_eq!(long.decode(&mut wire).unwrap().unwrap().value(), 0xbcd);
            if !short_first {
                assert_eq!(short.decode(&mut wire).unwrap().unwrap().value(), 0xa);
            }
            assert_eq!(&wire[..], b"raw");
        }
        let mut wire = BytesMut::from(&[0xaf][..]);
        assert_eq!(short.decode(&mut wire).unwrap().unwrap().value(), 0xa);
        assert!(wire.is_empty());
    }

    #[test]
    fn finite_eof_keeps_truncation_typed_and_retains_input() {
        let mut codec = BinCodec::<u16>::new();
        let mut wire = BytesMut::from(&[1][..]);
        assert_eq!(codec.decode(&mut wire).unwrap(), None);
        let error = bnb::BitError::from(codec.decode_eof(&mut wire).unwrap_err());
        assert_eq!(
            error.kind,
            bnb::ErrorKind::UnexpectedEof {
                needed: 16,
                remaining: 8
            }
        );
        assert_eq!(&wire[..], &[1]);
        wire.extend_from_slice(&[2]);
        assert_eq!(codec.decode_eof(&mut wire).unwrap(), Some(0x0102));
        assert_eq!(codec.decode_eof(&mut wire).unwrap(), None);
    }

    #[test]
    fn zero_width_frame_cannot_stall_a_drain_loop() {
        let mut codec = BinCodec::<bnb::UInt<u8, 0>>::new();
        let mut wire = BytesMut::from(&[1][..]);
        assert_eq!(
            bnb::BitError::from(codec.decode(&mut wire).unwrap_err()).kind,
            bnb::ErrorKind::NoProgress
        );
        assert_eq!(&wire[..], &[1]);
    }

    /// A fixed 4-byte message — its length is implicit in its `#[bin]` structure.
    #[bin(big)]
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Msg {
        a: u16,
        b: u16,
    }

    /// A magic-prefixed message — wrong bytes make `decode` a hard error, not "read more".
    #[bin(big, magic = 0xCAFEu16)]
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Magic {
        v: u8,
    }

    /// A **little-endian** message — the Decoder must read in the same order the Encoder
    /// writes, or the round-trip silently corrupts.
    #[bin(little)]
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct LeMsg {
        a: u16,
        b: u32,
    }

    #[test]
    fn little_endian_message_round_trips() {
        let mut codec = BinCodec::<LeMsg>::new();
        let mut buf = BytesMut::new();
        let m = LeMsg {
            a: 0x1234,
            b: 0xAABB_CCDD,
        };
        codec.encode(m.clone(), &mut buf).unwrap();
        // Little-endian on the wire.
        assert_eq!(&buf[..], &[0x34, 0x12, 0xDD, 0xCC, 0xBB, 0xAA]);
        // And the Decoder reads it back in the same order.
        assert_eq!(codec.decode(&mut buf).unwrap(), Some(m));
        assert!(buf.is_empty());
    }

    #[test]
    fn encoder_writes_exact_message_bytes() {
        let mut codec = BinCodec::<Msg>::new();
        let mut buf = BytesMut::new();
        codec
            .encode(
                Msg {
                    a: 0x0102,
                    b: 0x0304,
                },
                &mut buf,
            )
            .unwrap();
        assert_eq!(&buf[..], &[0x01, 0x02, 0x03, 0x04]);
    }

    #[test]
    fn encode_then_decode_round_trips_and_drains() {
        let mut codec = BinCodec::<Msg>::new();
        let mut buf = BytesMut::new();
        let m = Msg {
            a: 0xAABB,
            b: 0xCCDD,
        };
        codec.encode(m.clone(), &mut buf).unwrap();
        assert_eq!(codec.decode(&mut buf).unwrap(), Some(m));
        assert!(buf.is_empty(), "decode consumed exactly the one frame");
    }

    #[test]
    fn decode_empty_buffer_is_none() {
        let mut codec = BinCodec::<Msg>::new();
        let mut buf = BytesMut::new();
        assert_eq!(codec.decode(&mut buf).unwrap(), None);
    }

    #[test]
    fn decode_partial_frame_is_none_and_keeps_bytes() {
        let mut codec = BinCodec::<Msg>::new();
        let mut buf = BytesMut::from(&[0x01, 0x02][..]); // only 2 of the 4 bytes
        assert_eq!(codec.decode(&mut buf).unwrap(), None);
        assert_eq!(
            &buf[..],
            &[0x01, 0x02],
            "a partial frame is left for the next read"
        );
    }

    #[test]
    fn decode_consumes_one_message_and_leaves_the_tail() {
        let mut codec = BinCodec::<Msg>::new();
        let mut buf = BytesMut::from(&[0x01, 0x02, 0x03, 0x04, 0xEE, 0xFF][..]);
        assert_eq!(
            codec.decode(&mut buf).unwrap(),
            Some(Msg {
                a: 0x0102,
                b: 0x0304
            })
        );
        assert_eq!(
            &buf[..],
            &[0xEE, 0xFF],
            "trailing bytes remain for the next frame"
        );
    }

    #[test]
    fn decode_walks_back_to_back_messages() {
        let mut codec = BinCodec::<Msg>::new();
        let mut buf = BytesMut::from(&[0x00, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04][..]);
        assert_eq!(codec.decode(&mut buf).unwrap(), Some(Msg { a: 1, b: 2 }));
        assert_eq!(codec.decode(&mut buf).unwrap(), Some(Msg { a: 3, b: 4 }));
        assert_eq!(codec.decode(&mut buf).unwrap(), None);
    }

    #[test]
    fn decode_bad_magic_is_an_invalid_data_error() {
        let mut codec = BinCodec::<Magic>::new();
        let mut buf = BytesMut::from(&[0x00, 0x00, 0x07][..]); // full frame, wrong magic
        let err = codec.decode(&mut buf).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn default_constructs_a_codec() {
        let _c: BinCodec<Msg> = BinCodec::default();
    }
}
