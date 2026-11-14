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
//! without per-message padding, use [`BitBuf`] directly.
//!
//! `decode` leaves the entire buffer unchanged on incomplete input or errors. `decode_eof`
//! makes a finite attempt so truncation is an error. Parsing is replayed on each attempt;
//! callbacks must be retry-safe. Use `Framed::into_parts` / `Framed::from_parts` for protocol
//! transitions, transferring **both** read and write buffers; `into_inner` discards them.
//! This convenience codec uses `T::LAYOUT` from `BitEncode`; directional/contextual codecs
//! instead use [`BitBuf::try_pull_with`] with an explicit layout.
//!
//! **Frame size.** [`BinCodec::new`] takes whatever the message's own grammar asks for, so a
//! peer that announces a huge `count` makes `Framed` keep buffering. When the peer is
//! untrusted, use [`BinCodec::with_max_frame`]: a message longer than the cap fails with
//! [`ErrorKind::BufferFull`], as soon as that many bytes are buffered without completing it,
//! or when it decodes whole from one larger read. The cap bounds each message's bytes, not
//! what decoding allocates; `Framed`'s buffer can pass it by one read before `decode` runs.
//!
//! The same `BinCodec` drives **datagrams**, too: `tokio_util::udp::UdpFramed::new(udp_socket,
//! BinCodec::<T>::new())` is a `Stream<Item = (T, SocketAddr)>` + `Sink<(T, SocketAddr)>`. So
//! one codec covers async streams (`Framed`, TCP) and async datagrams (`UdpFramed`, UDP) — the
//! async mirror of the sync `MessageStream` / `MessageDatagram` split. See `examples/tokio_udp`.

use crate::bitstream::FrameWriter;
use crate::{BitDecode, BitEncode, BitError, ErrorKind};
use bytes::{Buf, BytesMut};
use core::fmt;
use core::marker::PhantomData;
use std::io;
use tokio_util::codec::{Decoder, Encoder};

/// A [`tokio_util::codec`] codec that frames any bnb `#[bin]` message `T`. Construct with
/// [`BinCodec::new`] and hand it to [`Framed`](tokio_util::codec::Framed).
pub struct BinCodec<T> {
    // `usize::MAX` is no cap: no buffer reaches it.
    max_frame: usize,
    message: PhantomData<T>,
}

impl<T> BinCodec<T> {
    /// A codec for messages of type `T`, with no maximum frame size.
    #[must_use]
    pub fn new() -> Self {
        Self::with_max_frame(usize::MAX)
    }

    /// A codec for messages of type `T` that rejects any message longer than `bytes`.
    ///
    /// `decode` and `decode_eof` fail with [`ErrorKind::BufferFull`] (`cap` is `bytes`) when a
    /// message spans more than `bytes`: once `bytes` are buffered and it has not ended (still
    /// incomplete, or truncated at EOF, as for a `UdpFramed` datagram), or when it decodes
    /// whole from a larger read. So the answer does not depend on how the input was split
    /// into reads. Errors the message raises inside the cap keep their own kind. The buffer
    /// is left unchanged, and bytes after the message do not count. A cap of 0 rejects every
    /// message.
    ///
    /// ```
    /// # use bnb::{BinCodec, ErrorKind, bin};
    /// # use bytes::BytesMut;
    /// # use tokio_util::codec::Decoder;
    /// #[bin]
    /// #[derive(Debug)]
    /// struct Blob {
    ///     n: u8,
    ///     #[br(count = n)]
    ///     data: Vec<u8>,
    /// }
    ///
    /// let mut codec = BinCodec::<Blob>::with_max_frame(16);
    /// let mut wire = BytesMut::from(&[200; 16][..]); // announces 200 bytes; 16 arrived
    /// let err = bnb::BitError::from(codec.decode(&mut wire).unwrap_err());
    /// assert_eq!(err.kind, ErrorKind::BufferFull { cap: 16 });
    /// ```
    #[must_use]
    pub fn with_max_frame(bytes: usize) -> Self {
        Self {
            max_frame: bytes,
            message: PhantomData,
        }
    }
}

impl<T> Default for BinCodec<T> {
    fn default() -> Self {
        Self::new()
    }
}

// Hand-written: a derive would demand the same trait of the message type `T`, which the
// stateless codec never stores.
impl<T> Clone for BinCodec<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for BinCodec<T> {}

impl<T> fmt::Debug for BinCodec<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut f = f.debug_struct("BinCodec");
        if self.max_frame != usize::MAX {
            f.field("max_frame", &self.max_frame);
        }
        f.finish()
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
        self.attempt(src, false)
    }

    fn decode_eof(&mut self, src: &mut BytesMut) -> Result<Option<T>, io::Error> {
        self.attempt(src, true)
    }
}

impl<T: BitDecode + BitEncode> BinCodec<T> {
    fn attempt(self, src: &mut BytesMut, eof: bool) -> Result<Option<T>, io::Error> {
        if src.is_empty() {
            return Ok(None);
        }
        let result = crate::bitstream::decode_attempt(&src[..], 0, T::LAYOUT, (), eof);
        match result {
            Ok((item, end)) if end.div_ceil(8) <= self.max_frame => {
                src.advance(end.div_ceil(8));
                Ok(Some(item))
            }
            Ok(_) => Err(self.frame_too_long()),
            // The cap is buffered and the message has not ended: it is longer than the cap.
            Err(e) if src.len() >= self.max_frame && ran_out(&e, eof) => Err(self.frame_too_long()),
            // Only a partial frame is buffered — ask `Framed` to read more (don't consume).
            Err(e) if e.is_incomplete() => Ok(None),
            // A genuine framing error.
            Err(e) => Err(io::Error::new(io::ErrorKind::InvalidData, e)),
        }
    }

    #[cold]
    #[inline(never)]
    fn frame_too_long(self) -> io::Error {
        let cap = self.max_frame;
        let err = BitError::new(ErrorKind::BufferFull { cap }, cap.saturating_mul(8));
        io::Error::new(io::ErrorKind::InvalidData, err)
    }
}

/// Whether an attempt stopped for want of input: `Incomplete`, or truncation at a finite EOF
/// (`UdpFramed` calls only `decode_eof`).
fn ran_out(e: &BitError, eof: bool) -> bool {
    e.is_incomplete()
        || eof
            && matches!(
                e.kind,
                ErrorKind::UnexpectedEof { .. } | ErrorKind::IncompleteAtEof { .. }
            )
}

impl<T: BitEncode> Encoder<T> for BinCodec<T> {
    type Error = io::Error;

    // Straight into `dst`, not through a `Vec` copied over afterwards. The writer takes a
    // message back out unless it is kept, so one that fails or panics part-way leaves `dst`
    // holding whole messages only.
    fn encode(&mut self, item: T, dst: &mut BytesMut) -> Result<(), io::Error> {
        let mut frame = FrameWriter::new(dst, T::LAYOUT);
        item.bit_encode(&mut frame)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        frame.keep();
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
    fn the_codec_is_copy_and_debug_whatever_the_message_type_is() {
        // A message type with none of these traits must not take them away from its codec.
        struct Opaque;
        let codec = BinCodec::<Opaque>::new();
        let copy = codec;
        assert_eq!(format!("{codec:?} {copy:?}"), "BinCodec BinCodec");
        let capped = BinCodec::<Opaque>::with_max_frame(9);
        assert_eq!(format!("{capped:?}"), "BinCodec { max_frame: 9 }");
        // A copy keeps the cap.
        #[expect(clippy::clone_on_copy, reason = "the `Clone` impl is what is tested")]
        let cloned = capped.clone();
        assert_eq!(format!("{cloned:?}"), "BinCodec { max_frame: 9 }");
    }

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

    /// Sub-byte fields, a counted run of two-byte integers and a counted byte run: every
    /// kind of write the in-place encoder makes.
    #[bin(big)]
    #[derive(Clone)]
    struct Varied {
        kind: bnb::u4,
        flags: bnb::u12,
        #[brw(count_prefix = u8)]
        words: Vec<u16>,
        #[brw(count_prefix = u8)]
        body: Vec<u8>,
    }

    #[test]
    fn encoder_appends_the_messages_own_bytes_after_what_the_buffer_holds() {
        let message = Varied {
            kind: bnb::u4::new(0xA),
            flags: bnb::u12::new(0xBCD),
            words: vec![0x0102, 0x0304, 0x0506],
            body: vec![0x77; 40],
        };
        let expected = message.to_bytes().unwrap();
        let mut codec = BinCodec::<Varied>::new();
        let mut buf = BytesMut::from(&b"earlier"[..]);
        codec.encode(message.clone(), &mut buf).unwrap();
        codec.encode(message, &mut buf).unwrap();
        assert_eq!(&buf[..7], b"earlier");
        assert_eq!(&buf[7..7 + expected.len()], &expected[..]);
        assert_eq!(&buf[7 + expected.len()..], &expected[..]);
    }

    #[test]
    fn a_message_that_fails_to_encode_leaves_the_buffer_as_it_was() {
        // The run is too long for its one-byte count: the error comes after the first
        // sixteen bits and the first run are written.
        let message = Varied {
            kind: bnb::u4::new(1),
            flags: bnb::u12::new(2),
            words: vec![9; 3],
            body: vec![0; 300],
        };
        let direct = message.to_bytes().unwrap_err();
        let mut codec = BinCodec::<Varied>::new();
        let mut buf = BytesMut::from(&b"earlier"[..]);
        let error = codec.encode(message, &mut buf).unwrap_err();
        assert_eq!(&buf[..], b"earlier");
        // The same error as encoding it alone.
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(bit_error(&error), &direct);
    }

    /// The `BitError` a codec's `io::Error` carries.
    fn bit_error(error: &std::io::Error) -> &bnb::BitError {
        error.get_ref().unwrap().downcast_ref().unwrap()
    }

    /// Writes its byte, then fails or panics as the byte says: an encoder that gives up
    /// after it has written.
    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "the signature `write_with` calls"
    )]
    fn write_then_give_up<K: bnb::Sink>(value: &u8, w: &mut K) -> Result<(), bnb::BitError> {
        w.write(*value)?;
        match value {
            0xEE => Err(bnb::BitError::new(
                bnb::ErrorKind::NotByteAligned,
                w.bit_pos(),
            )),
            0xFF => panic!("an encoder that panics part-way"),
            _ => Ok(()),
        }
    }

    // Bit-packed LSB-first and little-endian, ending inside a byte.
    #[bin(little, bits = lsb)]
    #[derive(Clone)]
    struct Fragile {
        head: u32,
        flag: bnb::u4,
        #[bw(write_with = write_then_give_up)]
        last: u8,
        tail: bnb::u4,
    }

    #[test]
    fn an_encoder_that_errors_or_panics_after_writing_leaves_no_part_of_its_message() {
        let fragile = |last| Fragile {
            head: 0xDEAD_BEEF,
            flag: bnb::u4::new(0x9),
            last,
            tail: bnb::u4::new(0x6),
        };
        let mut codec = BinCodec::<Fragile>::new();
        let mut buf = BytesMut::from(&b"earlier"[..]);

        // An error raised five bytes in: the buffer is as it was, the error as it is alone.
        let direct = fragile(0xEE).to_bytes().unwrap_err();
        assert_eq!(direct.at, 44);
        let error = codec.encode(fragile(0xEE), &mut buf).unwrap_err();
        assert_eq!(bit_error(&error), &direct);
        assert_eq!(&buf[..], b"earlier");

        // A panic at the same point, caught by whoever owns the buffer.
        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            codec.encode(fragile(0xFF), &mut buf)
        }));
        assert!(unwound.is_err());
        assert_eq!(&buf[..], b"earlier");

        // And the buffer still takes a whole message, in this layout, after both.
        let expected = fragile(7).to_bytes().unwrap();
        codec.encode(fragile(7), &mut buf).unwrap();
        assert_eq!(&buf[7..], &expected[..]);
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

    /// A length-prefixed message: one byte of count, then that many bytes.
    #[bin(big)]
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Var {
        n: u8,
        #[br(count = n)]
        data: Vec<u8>,
    }

    fn frame(n: u8) -> Vec<u8> {
        let mut wire = vec![n];
        wire.extend(core::iter::repeat_n(0xAB, n.into()));
        wire
    }

    fn buffer_full(err: std::io::Error) -> (bnb::ErrorKind, usize) {
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
        let err = bnb::BitError::from(err);
        (err.kind, err.at)
    }

    #[test]
    fn a_frame_of_exactly_the_cap_decodes() {
        let mut codec = BinCodec::<Var>::with_max_frame(5);
        let mut wire = BytesMut::from(&frame(4)[..]);
        assert_eq!(codec.decode(&mut wire).unwrap().unwrap().data.len(), 4);
        assert!(wire.is_empty());
    }

    #[test]
    fn an_incomplete_frame_below_the_cap_waits() {
        let mut codec = BinCodec::<Var>::with_max_frame(5);
        let mut wire = BytesMut::from(&frame(4)[..4]);
        assert_eq!(codec.decode(&mut wire).unwrap(), None);
        assert_eq!(wire.len(), 4);
    }

    #[test]
    fn an_incomplete_frame_at_the_cap_fails_and_keeps_the_buffer() {
        // Five bytes buffered and still incomplete: the frame is longer than five.
        let mut codec = BinCodec::<Var>::with_max_frame(5);
        let mut wire = BytesMut::from(&frame(200)[..5]);
        let (kind, at) = buffer_full(codec.decode(&mut wire).unwrap_err());
        assert_eq!(kind, bnb::ErrorKind::BufferFull { cap: 5 });
        assert_eq!(at, 5 * 8);
        assert_eq!(wire.len(), 5);
    }

    #[test]
    fn an_oversize_frame_fails_however_it_arrives() {
        // Whole in one read, or a byte at a time: the same frame gets the same answer.
        let mut codec = BinCodec::<Var>::with_max_frame(5);
        let mut wire = BytesMut::from(&frame(5)[..]);
        let (kind, at) = buffer_full(codec.decode(&mut wire).unwrap_err());
        assert_eq!(kind, bnb::ErrorKind::BufferFull { cap: 5 });
        assert_eq!(at, 5 * 8);
        assert_eq!(wire.len(), 6);
        let mut wire = BytesMut::new();
        let result = frame(5).iter().find_map(|byte| {
            wire.extend_from_slice(&[*byte]);
            codec.decode(&mut wire).transpose()
        });
        let (kind, _) = buffer_full(result.unwrap().unwrap_err());
        assert_eq!(kind, bnb::ErrorKind::BufferFull { cap: 5 });
    }

    #[test]
    fn truncation_at_eof_reports_the_cap_once_it_is_buffered() {
        // `UdpFramed` calls only `decode_eof`: a datagram as long as the cap that does not
        // hold the message is over the cap; a shorter one is truncated.
        let mut codec = BinCodec::<Var>::with_max_frame(5);
        let mut wire = BytesMut::from(&frame(200)[..5]);
        let (kind, _) = buffer_full(codec.decode_eof(&mut wire).unwrap_err());
        assert_eq!(kind, bnb::ErrorKind::BufferFull { cap: 5 });
        assert_eq!(wire.len(), 5);
        let mut wire = BytesMut::from(&frame(200)[..4]);
        let (kind, _) = buffer_full(codec.decode_eof(&mut wire).unwrap_err());
        assert!(
            matches!(kind, bnb::ErrorKind::UnexpectedEof { .. }),
            "{kind:?}"
        );
    }

    #[test]
    fn an_oversize_frame_at_eof_fails() {
        let mut codec = BinCodec::<Var>::with_max_frame(5);
        let mut wire = BytesMut::from(&frame(5)[..]);
        let (kind, _) = buffer_full(codec.decode_eof(&mut wire).unwrap_err());
        assert_eq!(kind, bnb::ErrorKind::BufferFull { cap: 5 });
        assert_eq!(wire.len(), 6);
    }

    #[test]
    fn errors_inside_the_cap_keep_their_kind() {
        // A full frame with the wrong magic, at and past a cap it does not fit.
        let mut codec = BinCodec::<Magic>::with_max_frame(2);
        for eof in [false, true] {
            let mut wire = BytesMut::from(&[0x00, 0x00, 0x07][..]);
            let err = if eof {
                codec.decode_eof(&mut wire)
            } else {
                codec.decode(&mut wire)
            };
            let (kind, _) = buffer_full(err.unwrap_err());
            assert!(matches!(kind, bnb::ErrorKind::BadMagic { .. }), "{kind:?}");
        }
    }

    #[test]
    fn a_sub_byte_frame_counts_its_padded_final_byte() {
        // 12 bits take two bytes on the wire.
        let mut wire = BytesMut::new();
        BinCodec::<bnb::u12>::new()
            .encode(bnb::u12::new(0xbcd), &mut wire)
            .unwrap();
        let mut one = BinCodec::<bnb::u12>::with_max_frame(1);
        let (kind, _) = buffer_full(one.decode(&mut wire.clone()).unwrap_err());
        assert_eq!(kind, bnb::ErrorKind::BufferFull { cap: 1 });
        let mut two = BinCodec::<bnb::u12>::with_max_frame(2);
        assert_eq!(two.decode(&mut wire).unwrap().unwrap().value(), 0xbcd);
        assert!(wire.is_empty());
    }

    #[test]
    fn bytes_after_the_frame_do_not_count() {
        let mut codec = BinCodec::<Var>::with_max_frame(2);
        let mut wire = BytesMut::from(&frame(1)[..]);
        wire.extend_from_slice(&frame(1));
        wire.extend_from_slice(&[0xFF; 64]);
        assert_eq!(codec.decode(&mut wire).unwrap().unwrap().data, [0xAB]);
        assert_eq!(codec.decode(&mut wire).unwrap().unwrap().data, [0xAB]);
        assert_eq!(wire.len(), 64);
    }

    #[test]
    fn the_default_codec_has_no_cap() {
        let mut codec = BinCodec::<Var>::new();
        let mut wire = BytesMut::from(&frame(255)[..255]);
        assert_eq!(codec.decode(&mut wire).unwrap(), None);
        wire.extend_from_slice(&[0xAB]);
        assert_eq!(codec.decode(&mut wire).unwrap().unwrap().data.len(), 255);
    }

    #[test]
    fn default_constructs_a_codec() {
        let _c: BinCodec<Msg> = BinCodec::default();
    }
}
