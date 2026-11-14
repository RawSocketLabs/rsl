//! The transports' `_with` forms: a message that does not encode, and one that decodes with
//! context, read through each transport in the layout its type declares.
#![cfg(feature = "mock")]

mod behaviour {
    use bnb::net::{MessageReadError, read_message_with};
    use bnb::{
        BitBuf, BitOrder, ByteOrder, ErrorKind, Layout, MessageDatagram, MessageStream,
        MockDatagramSocket, MockStream, bin,
    };

    /// Decode-only and little-endian: read big-endian, `value` comes out byte-swapped.
    #[bin(little, read_only)]
    #[derive(Debug, PartialEq)]
    struct Reading {
        sensor: u8,
        value: u16,
        #[brw(count_prefix = u8)]
        note: Vec<u8>,
    }

    #[bin(little, read_only)]
    #[derive(Debug, PartialEq)]
    enum Event {
        #[bin(magic = 1u8)]
        Up(u16),
        #[bin(magic = 2u8)]
        Down,
    }

    #[bin(bits = lsb, bytes = little, read_only)]
    #[derive(Debug, PartialEq)]
    struct Packed {
        low: bnb::u4,
        high: bnb::u4,
    }

    /// A body whose length the caller knows: it decodes with context and so through no
    /// plain `read_message`.
    #[bin(big, ctx(len: u8))]
    #[derive(Debug, PartialEq)]
    struct Body {
        kind: u16,
        #[br(count = len)]
        bytes: Vec<u8>,
    }

    const READING: [u8; 7] = [7, 0x34, 0x12, 3, b'a', b'b', b'c'];

    /// The order `Reading`, `Event` and `Tail` declare. A type that does not encode has
    /// no `LAYOUT` to ask, so the caller states it.
    const LITTLE: Layout = Layout {
        bit: BitOrder::Msb,
        byte: ByteOrder::Little,
    };

    /// The order `Packed` declares.
    const LSB_LITTLE: Layout = Layout {
        bit: BitOrder::Lsb,
        byte: ByteOrder::Little,
    };

    fn reading() -> Reading {
        Reading {
            sensor: 7,
            value: 0x1234,
            note: b"abc".to_vec(),
        }
    }

    #[test]
    fn a_stream_reads_a_decode_only_message_in_its_own_layout() {
        // A byte at a time, so each message takes several attempts.
        let mut transport = MockStream::with_chunk_size(1);
        transport.push_inbound(&READING);
        transport.push_inbound(&[1, 0x34, 0x12]);
        transport.push_inbound(&[2]);
        transport.push_inbound(&[0xA5]);
        let mut stream = MessageStream::new(transport);
        let first: Reading = stream.read_message_with(LITTLE, ()).unwrap();
        assert_eq!(first, reading());
        let event: Event = stream.read_message_with(LITTLE, ()).unwrap();
        assert_eq!(event, Event::Up(0x1234));
        let event: Event = stream.read_message_with(LITTLE, ()).unwrap();
        assert_eq!(event, Event::Down);
        let packed: Packed = stream.read_message_with(LSB_LITTLE, ()).unwrap();
        assert_eq!((packed.low.value(), packed.high.value()), (5, 0xA));
        // The stream has ended: nothing was read ahead and lost, nothing is left.
        let end = stream.read_message_with::<Event, _>(LITTLE, ());
        assert!(matches!(end, Err(MessageReadError::Io(_))), "{end:?}");
    }

    #[test]
    fn the_layout_given_is_the_one_decoded_in() {
        let mut wire = &READING[..];
        let swapped: Reading = read_message_with(
            &mut wire,
            &mut BitBuf::new(),
            &mut [0; 16],
            Layout::default(),
            (),
        )
        .unwrap();
        assert_eq!(swapped.value, 0x3412);
    }

    #[test]
    fn a_message_with_context_is_read_with_it_on_every_attempt() {
        // Three bytes at a time: the body is decoded again as it arrives, each time with
        // the context.
        struct Pieces<'a>(&'a [u8]);
        impl std::io::Read for Pieces<'_> {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                let count = out.len().min(3);
                self.0.read(&mut out[..count])
            }
        }

        let mut wire = Pieces(&[0xAB, 0xCD, 1, 2, 3, 4, 5, 0xFF]);
        let (mut buffer, mut scratch) = (BitBuf::new(), [0; 16]);
        let body: Body = read_message_with(
            &mut wire,
            &mut buffer,
            &mut scratch,
            Layout::default(),
            BodyCtx::new(5),
        )
        .unwrap();
        assert_eq!(
            body,
            Body {
                kind: 0xABCD,
                bytes: vec![1, 2, 3, 4, 5]
            }
        );
        // The byte that followed the body is still there: not a magic `Event` knows.
        let next =
            read_message_with::<Event, _, _>(&mut wire, &mut buffer, &mut scratch, LITTLE, ());
        assert!(matches!(next, Err(MessageReadError::Codec(_))), "{next:?}");
    }

    #[test]
    fn a_message_cut_short_by_the_end_of_the_stream_is_a_codec_error() {
        let mut wire = &READING[..5];
        let error = read_message_with::<Reading, _, _>(
            &mut wire,
            &mut BitBuf::new(),
            &mut [0; 16],
            LITTLE,
            (),
        )
        .unwrap_err();
        assert!(
            matches!(
                &error,
                MessageReadError::Codec(error)
                    if matches!(error.kind, ErrorKind::UnexpectedEof { .. })
            ),
            "{error:?}"
        );
    }

    /// Only the end of the input settles which variant two bytes are.
    #[bin(little, read_only)]
    #[derive(Debug, PartialEq)]
    enum Tail {
        #[bin(magic = b"ABCDE")]
        Long,
        Raw(u16),
    }

    #[test]
    fn a_message_the_end_of_the_stream_completes_is_decoded_in_the_layout_given() {
        // Both bytes could begin the magic, so only the end of the input decides.
        let mut wire: &[u8] = b"AB";
        let tail: Tail =
            read_message_with(&mut wire, &mut BitBuf::new(), &mut [0; 16], LITTLE, ()).unwrap();
        assert_eq!(tail, Tail::Raw(0x4241));
    }

    #[cfg(feature = "tokio-io")]
    #[tokio::test]
    async fn the_async_reader_decodes_what_the_end_of_the_stream_completes() {
        // Both bytes could begin the magic, so only the end of the input decides.
        let mut wire: &[u8] = b"AB";
        let tail: Tail = bnb::net::read_message_async_with(
            &mut wire,
            &mut BitBuf::new(),
            &mut [0; 16],
            LITTLE,
            (),
        )
        .await
        .unwrap();
        assert_eq!(tail, Tail::Raw(0x4241));
    }

    #[test]
    fn a_datagram_is_decoded_whole_in_the_layout_given() {
        let from = "127.0.0.1:5000".parse().unwrap();
        let mut peer = MessageDatagram::new(MockDatagramSocket::new());
        peer.get_ref().push_inbound(&READING, from);
        let (got, who) = peer.recv_message_with::<Reading, _>(LITTLE, ()).unwrap();
        assert_eq!((got, who), (reading(), from));

        peer.get_ref().push_inbound(&[0x00, 0x09, 7, 7], from);
        let (body, _) = peer
            .recv_message_with::<Body, _>(Layout::default(), BodyCtx::new(2))
            .unwrap();
        assert_eq!(body.bytes, [7, 7]);

        // A datagram is one message: bytes after it are an error, as for `recv_message`.
        peer.get_ref().push_inbound(&[0x00, 0x09, 7, 7], from);
        let error = peer
            .recv_message_with::<Body, _>(Layout::default(), BodyCtx::new(1))
            .unwrap_err();
        assert!(
            matches!(error.kind, ErrorKind::TrailingBytes { .. }),
            "{error:?}"
        );
    }

    #[cfg(feature = "tokio-io")]
    #[tokio::test]
    async fn the_async_reader_takes_the_same_layout_and_context() {
        use bnb::net::read_message_async_with;

        let mut wire = &READING[..];
        let (mut buffer, mut scratch) = (BitBuf::new(), [0; 2]);
        let got: Reading =
            read_message_async_with(&mut wire, &mut buffer, &mut scratch, LITTLE, ())
                .await
                .unwrap();
        assert_eq!(got, reading());

        let mut wire: &[u8] = &[0xAB, 0xCD, 9];
        let body: Body = read_message_async_with(
            &mut wire,
            &mut buffer,
            &mut scratch,
            Layout::default(),
            BodyCtx::new(1),
        )
        .await
        .unwrap();
        assert_eq!(body.bytes, [9]);
    }
}
