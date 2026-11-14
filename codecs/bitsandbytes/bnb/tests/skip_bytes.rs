//! `Source::skip_bytes` against `read_bytes` with the bytes dropped: through every reader,
//! from byte boundaries and from inside a byte, for runs that are there and runs that are
//! not.

mod component {
    use bnb::{
        BitBuf, BitError, BitReader, BufSeekReader, BufSource, ErrorKind, SeekReader, Source,
        StreamBitReader, bin, u4,
    };
    use std::io::{Cursor, Read};

    /// 700 bytes, no two neighbours equal, so a cursor a byte off reads another value.
    fn wire() -> Vec<u8> {
        (0..700_u32)
            .map(|at| u8::try_from((at * 7 + at / 251) % 251).unwrap())
            .collect()
    }

    /// A reader that gives at most `chunk` bytes a call, as a socket does.
    struct Dribble<'a>(&'a [u8], usize);

    impl Read for Dribble<'_> {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            let limit = out.len().min(self.1);
            self.0.read(&mut out[..limit])
        }
    }

    /// From `start_bits` into the source: `skip_bytes(n)` succeeds where `read_bytes(n)`
    /// does and fails with its error, and either way both stand at the same bit and read
    /// the same next byte.
    fn assert_skips_as_it_reads<S: Source>(make: impl Fn() -> S, context: &str) {
        for start_bits in [0, 3, 8, 13] {
            for n in [0, 1, 2, 255, 256, 257, 511, 513, 698, 699, 700, 701, 5_000] {
                let context = format!("{context}: {n} bytes from bit {start_bits}");
                let (mut skips, mut reads) = (make(), make());
                skips.read_bits(start_bits).unwrap();
                reads.read_bits(start_bits).unwrap();
                let skipped = skips.skip_bytes(n);
                let read = reads.read_bytes(n);
                assert_eq!(skipped, read.map(drop), "{context}");
                assert_eq!(skips.bit_pos(), reads.bit_pos(), "{context}");
                assert_eq!(
                    skips.read::<u8>().ok(),
                    reads.read::<u8>().ok(),
                    "{context}: the byte after"
                );
            }
        }
    }

    #[test]
    fn a_skip_stops_where_a_read_of_the_same_bytes_stops_on_every_reader() {
        let wire = wire();
        assert_skips_as_it_reads(|| BitReader::new(&wire), "BitReader");
        assert_skips_as_it_reads(
            || {
                let mut buffer = BitBuf::new();
                buffer.push(&wire).unwrap();
                buffer
            },
            "BitBuf",
        );
        for chunk in [1, 7, 300, usize::MAX] {
            assert_skips_as_it_reads(
                || StreamBitReader::new(Dribble(&wire, chunk)),
                &format!("StreamBitReader, {chunk} a read"),
            );
            assert_skips_as_it_reads(
                || BufSource::new(Dribble(&wire, chunk)),
                &format!("BufSource, {chunk} a read"),
            );
        }
        assert_skips_as_it_reads(|| SeekReader::new(Cursor::new(&wire[..])), "SeekReader");
        assert_skips_as_it_reads(
            || BufSeekReader::new(Cursor::new(&wire[..])),
            "BufSeekReader",
        );
        #[cfg(feature = "bytes")]
        assert_skips_as_it_reads(|| bnb::BytesReader::new(wire.clone().into()), "BytesReader");
    }

    #[test]
    fn a_skip_cannot_leave_a_region_and_a_region_skipped_through_is_finished() {
        let wire = wire();
        for (start, len) in [(0, 300), (2, 300), (0, 700), (10, 1)] {
            // Inside the region: as on the reader itself.
            for n in [0, 1, len - 1, len] {
                let mut reader = BitReader::new(&wire);
                reader.skip_bytes(start).unwrap();
                let mut region = reader.limit_bytes(len).unwrap();
                let context = format!("{n} of a region of {len} at byte {start}");
                region.skip_bytes(n).unwrap();
                assert_eq!(region.remaining_bits(), (len - n) * 8, "{context}");
                assert_eq!(region.finish().is_ok(), n == len, "{context}");
                assert_eq!(reader.bit_pos(), (start + n) * 8, "{context}");
            }
            // Past its end: refused, though the reader behind it has the bytes.
            let mut reader = BitReader::new(&wire);
            reader.skip_bytes(start).unwrap();
            let mut region = reader.limit_bytes(len).unwrap();
            let error = region.skip_bytes(len + 1).unwrap_err();
            assert!(
                matches!(error.kind, ErrorKind::UnexpectedEof { .. }),
                "{error:?}"
            );
        }
        // The same through a reader that cannot seek: to the edge, and not past it.
        let mut reader = StreamBitReader::new(Dribble(&wire, 7));
        let mut region = reader.limit_bytes(600).unwrap();
        region.skip_bytes(600).unwrap();
        region.finish().unwrap();
        assert_eq!(reader.read::<u8>().unwrap(), wire[600]);
        let mut reader = StreamBitReader::new(Dribble(&wire, 7));
        let mut region = reader.limit_bytes(600).unwrap();
        let error = region.skip_bytes(601).unwrap_err();
        assert!(
            matches!(error.kind, ErrorKind::UnexpectedEof { .. }),
            "{error:?}"
        );
    }

    #[test]
    fn a_reader_that_keeps_what_it_reads_keeps_what_it_skips_up_to_its_cap() {
        let wire = wire();
        // Within the cap: skipped, and still there to seek back to.
        let mut kept = BufSource::with_capacity(&wire[..], 700);
        kept.skip_bytes(699).unwrap();
        kept.seek_to_bit(8).unwrap();
        assert_eq!(kept.read::<u8>().unwrap(), wire[1]);
        // Past it: refused though the bytes are there, as a read of them is.
        let mut small = BufSource::with_capacity(&wire[..], 300);
        let error = small.skip_bytes(301).unwrap_err();
        assert_eq!(error.kind, ErrorKind::BufferFull { cap: 300 });
    }

    #[test]
    fn a_length_no_input_backs_is_an_error_at_once() {
        let mut reader = BitReader::new(&[1, 2]);
        let error = reader.skip_bytes(usize::MAX).unwrap_err();
        assert!(matches!(error.kind, ErrorKind::UnexpectedEof { .. }));
        let mut stream = StreamBitReader::new(&[1, 2][..]);
        assert!(stream.skip_bytes(usize::MAX).is_err());
    }

    fn skips_a_body<S: Source>(r: &mut S) -> Result<u16, BitError> {
        let len: u16 = r.read()?;
        r.skip_bytes(usize::from(len))?;
        Ok(len)
    }

    fn reads_a_body<S: Source>(r: &mut S) -> Result<u16, BitError> {
        let len: u16 = r.read()?;
        r.read_bytes(usize::from(len))?;
        Ok(len)
    }

    /// A length, a body nobody parses, and a byte after it.
    #[bin(big, read_only)]
    #[derive(Debug, PartialEq)]
    struct Skipped {
        #[br(parse_with = skips_a_body)]
        len: u16,
        after: u8,
    }

    #[bin(big, read_only)]
    #[derive(Debug, PartialEq)]
    struct Kept {
        #[br(parse_with = reads_a_body)]
        len: u16,
        after: u8,
    }

    /// The same after four bits, so the body starts and ends inside a byte.
    fn skips_from_inside_a_byte<S: Source>(r: &mut S) -> Result<u16, BitError> {
        r.read::<u4>()?;
        skips_a_body(r)
    }

    fn reads_from_inside_a_byte<S: Source>(r: &mut S) -> Result<u16, BitError> {
        r.read::<u4>()?;
        reads_a_body(r)
    }

    /// The body in a region of its length, skipped to the region's end.
    fn skips_a_region<S: Source>(r: &mut S) -> Result<u16, BitError> {
        let len: u16 = r.read()?;
        let mut region = r.limit_bytes(usize::from(len))?;
        region.skip_bytes(usize::from(len))?;
        region.finish()?;
        Ok(len)
    }

    #[bin(big, read_only)]
    #[derive(Debug, PartialEq)]
    struct SkippedUnaligned {
        #[br(parse_with = skips_from_inside_a_byte)]
        len: u16,
        after: u4,
    }

    #[bin(big, read_only)]
    #[derive(Debug, PartialEq)]
    struct KeptUnaligned {
        #[br(parse_with = reads_from_inside_a_byte)]
        len: u16,
        after: u4,
    }

    #[bin(big, read_only)]
    #[derive(Debug, PartialEq)]
    struct SkippedRegion {
        #[br(parse_with = skips_a_region)]
        len: u16,
        after: u8,
    }

    #[test]
    fn a_skip_short_of_its_bytes_asks_for_all_it_lacks_as_a_read_does() {
        // 5,000 bytes announced, cut everywhere a transport might cut them.
        let mut message = vec![0x13, 0x88];
        message.extend(wire().iter().cycle().take(5_000));
        message.push(0xAF);
        for held in [0, 1, 2, 3, 100, 4_999, 5_001, 5_002] {
            let skipped = Skipped::decode_prefix(&message[..held]).unwrap_err();
            let read = Kept::decode_prefix(&message[..held]).unwrap_err();
            assert!(skipped.is_incomplete(), "{held} held: {skipped:?}");
            assert_eq!(skipped, read, "{held} held");
        }
        // With two bytes held the whole body is known to be missing, and said so at once.
        assert_eq!(
            Skipped::decode_prefix(&message[..2]).unwrap_err().kind,
            ErrorKind::Incomplete {
                needed: Some(5_000)
            }
        );
        let (whole, used) = Skipped::decode_prefix(&message).unwrap();
        assert_eq!((whole.len, whole.after, used), (5_000, 0xAF, message.len()));

        // From inside a byte: the same signal, the same bit, at every cut.
        let mut unaligned = vec![0x01, 0x38, 0x80]; // four bits, then 0x1388 = 5,000
        unaligned.extend(wire().iter().cycle().take(5_000));
        for held in [0, 1, 2, 3, 4, 100, 5_001, 5_002] {
            let skipped = SkippedUnaligned::decode_prefix(&unaligned[..held]);
            let read = KeptUnaligned::decode_prefix(&unaligned[..held]);
            let (skipped, read) = (skipped.unwrap_err(), read.unwrap_err());
            assert!(skipped.is_incomplete(), "{held} held: {skipped:?}");
            assert_eq!(skipped, read, "{held} held");
        }

        // From inside a region the signal is still the whole shortfall, where a read in
        // a region asks for the first missing byte.
        assert_eq!(
            SkippedRegion::decode_prefix(&message[..102])
                .unwrap_err()
                .kind,
            ErrorKind::Incomplete {
                needed: Some(4_900)
            }
        );
        let (whole, used) = SkippedRegion::decode_prefix(&message).unwrap();
        assert_eq!((whole.len, whole.after, used), (5_000, 0xAF, message.len()));
        // And off a buffer fed a piece at a time.
        let mut buffer = BitBuf::new();
        let mut seen = None;
        for piece in message.chunks(613) {
            assert_eq!(seen, None);
            buffer.push(piece).unwrap();
            seen = buffer
                .try_pull_with::<Skipped, ()>(bnb::Layout::default(), ())
                .ok();
        }
        assert_eq!(seen.map(|skipped| skipped.after), Some(0xAF));
    }
}
