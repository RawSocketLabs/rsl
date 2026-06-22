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
