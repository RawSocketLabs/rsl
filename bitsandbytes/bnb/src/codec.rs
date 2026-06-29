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
//! The same `BinCodec` drives **datagrams**, too: `tokio_util::udp::UdpFramed::new(udp_socket,
//! BinCodec::<T>::new())` is a `Stream<Item = (T, SocketAddr)>` + `Sink<(T, SocketAddr)>`. So
//! one codec covers async streams (`Framed`, TCP) and async datagrams (`UdpFramed`, UDP) — the
//! async mirror of the sync `MessageStream` / `MessageDatagram` split. See `examples/tokio_udp`.

use crate::{BitDecode, BitEncode, BitReader, BitWriter, ErrorKind};
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

impl<T: BitDecode> Decoder for BinCodec<T> {
    type Item = T;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<T>, io::Error> {
        if src.is_empty() {
            return Ok(None);
        }
        let mut reader = BitReader::new(&src[..]);
        match <T as BitDecode>::bit_decode(&mut reader) {
            Ok(item) => {
                // Consume exactly what this message used; leave the rest for the next call.
                let consumed = reader.bit_pos() / 8;
                src.advance(consumed);
                Ok(Some(item))
            }
            // Only a partial frame is buffered — ask `Framed` to read more (don't consume).
            Err(e)
                if matches!(
                    e.kind,
                    ErrorKind::UnexpectedEof { .. } | ErrorKind::Incomplete { .. }
                ) =>
            {
                Ok(None)
            }
            // A genuine framing error.
            Err(e) => Err(io::Error::new(io::ErrorKind::InvalidData, e.to_string())),
        }
    }
}

impl<T: BitEncode> Encoder<T> for BinCodec<T> {
    type Error = io::Error;

    fn encode(&mut self, item: T, dst: &mut BytesMut) -> Result<(), io::Error> {
        let mut w = BitWriter::with_layout(<T as BitEncode>::LAYOUT);
        item.bit_encode(&mut w)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
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
