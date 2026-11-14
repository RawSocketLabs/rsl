use crate::bitstream::{CountPrefix, decode_attempt, decode_prefix, decode_prefix_with};
use crate::{BitBuf, BitDecode, BitError, DecodeWith, ErrorKind, Layout, Source, bin};

#[bin(big)]
#[derive(Debug, PartialEq, Eq)]
struct Packet {
    tag: u8,
    value: u32,
}

#[bin(big)]
#[derive(Debug, PartialEq, Eq)]
struct Blob {
    #[brw(count_prefix = u16)]
    body: Vec<u8>,
}

#[test]
fn an_attempt_starts_where_it_is_told_and_rejects_a_start_past_the_end() {
    let wire = [0xFF, 7, 0, 0, 0, 9];
    for eof in [false, true] {
        // From bit 8: the message, and where it ended.
        let (packet, end) =
            decode_attempt::<Packet, ()>(&wire, 8, Layout::default(), (), eof).unwrap();
        assert_eq!(packet, Packet { tag: 7, value: 9 });
        assert_eq!(end, 48);
        // Past the end: the seek's own error, from bit 0, whatever `eof` says.
        let error =
            decode_attempt::<Packet, ()>(&wire, 49, Layout::default(), (), eof).unwrap_err();
        assert_eq!(
            error.kind,
            ErrorKind::UnexpectedEof {
                needed: 49,
                remaining: 48
            }
        );
        assert_eq!(error.at, 0);
        // At the end exactly is in range; nothing is there to read, which is a wait
        // or a truncation as `eof` says.
        let error =
            decode_attempt::<Packet, ()>(&wire, 48, Layout::default(), (), eof).unwrap_err();
        assert_eq!(error.at, 48);
        assert_eq!(error.is_incomplete(), !eof, "{:?}", error.kind);
        let truncated = matches!(error.kind, ErrorKind::UnexpectedEof { .. });
        assert_eq!(truncated, eof, "{:?}", error.kind);
    }
}

#[test]
fn shortfall_reports_next_operation_and_keeps_input() {
    let mut buffer = BitBuf::new();
    assert_eq!(
        buffer.try_pull::<Packet>().unwrap_err().kind,
        ErrorKind::Incomplete { needed: None }
    );
    buffer.push(&[7, 1, 2]).unwrap();
    let error = buffer.try_pull::<Packet>().unwrap_err();
    assert_eq!(error.kind, ErrorKind::Incomplete { needed: Some(2) });
    assert_eq!((error.at, error.field), (8, Some("value")));
    assert_eq!(buffer.bit_len(), 24);
    assert_eq!(buffer.try_pull::<Packet>().unwrap_err(), error);
    buffer.push(&[3, 4, 9]).unwrap();
    assert_eq!(
        buffer.try_pull::<Packet>().unwrap(),
        Packet {
            tag: 7,
            value: 0x0102_0304
        }
    );
    assert_eq!(buffer.try_pull::<u8>().unwrap(), 9);
    assert!(buffer.is_empty());
}

#[test]
fn eof_is_finite_per_attempt_not_sticky() {
    let mut buffer = BitBuf::new();
    assert_eq!(buffer.pull_eof::<Packet>().unwrap(), None);
    buffer.push(&[7, 1, 2]).unwrap();
    let error = buffer.pull_eof::<Packet>().unwrap_err();
    assert_eq!(
        error.kind,
        ErrorKind::UnexpectedEof {
            needed: 32,
            remaining: 16
        }
    );
    assert_eq!((error.at, error.field), (8, Some("value")));
    assert_eq!(buffer.bit_len(), 24);
    buffer.push(&[3, 4]).unwrap();
    assert!(buffer.pull_eof::<Packet>().unwrap().is_some());
    assert!(buffer.is_empty());
}

#[test]
fn byte_blob_waits_for_entire_body_and_keeps_trailing_frame() {
    let mut buffer = BitBuf::new();
    buffer.push(&[0, 4, 1]).unwrap();
    let error = buffer.try_pull::<Blob>().unwrap_err();
    assert_eq!(error.kind, ErrorKind::Incomplete { needed: Some(3) });
    assert_eq!((error.at, error.field), (16, Some("body")));
    buffer.push(&[2, 3, 4, 0, 0]).unwrap();
    assert_eq!(buffer.try_pull::<Blob>().unwrap().body, [1, 2, 3, 4]);
    assert_eq!(buffer.try_pull::<Blob>().unwrap().body, b"");
    assert!(buffer.is_empty());
}

#[derive(Debug)]
struct Region;
impl DecodeWith<()> for Region {
    fn decode_with<S: Source>(source: &mut S, (): ()) -> Result<Self, BitError> {
        let mut region = source.limit_bytes(1)?;
        region.read::<u16>().map_err(|e| e.in_field("inner"))?;
        Ok(Self)
    }
}

#[derive(Debug)]
struct WindowValue(u32);
impl DecodeWith<usize> for WindowValue {
    fn decode_with<S: Source>(source: &mut S, length: usize) -> Result<Self, BitError> {
        source
            .limit_bytes(length)?
            .read::<u32>()
            .map(Self)
            .map_err(|error| error.in_field("window"))
    }
}

#[test]
fn context_window_preserves_physical_hints_but_not_logical_exhaustion() {
    let mut buffer = BitBuf::new();
    buffer.push(&[1]).unwrap();
    let error = buffer
        .try_pull_with::<WindowValue, _>(Layout::default(), 4)
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Incomplete { needed: Some(3) });
    assert_eq!(error.field, Some("window"));
    for count in 0..3 {
        let mut extended = buffer.clone();
        extended.push(&vec![0xff; count]).unwrap();
        assert!(
            extended
                .try_pull_with::<WindowValue, _>(Layout::default(), 4)
                .unwrap_err()
                .is_incomplete()
        );
    }
    assert!(matches!(
        buffer
            .try_pull_with::<WindowValue, _>(Layout::default(), 3)
            .unwrap_err()
            .kind,
        ErrorKind::UnexpectedEof {
            needed: 32,
            remaining: 24
        }
    ));
    buffer.push(&[2, 3, 4]).unwrap();
    assert_eq!(
        buffer
            .try_pull_with::<WindowValue, _>(Layout::default(), 4)
            .unwrap()
            .0,
        0x0102_0304
    );
}

#[derive(Debug)]
struct Seeking;
impl DecodeWith<()> for Seeking {
    fn decode_with<S: Source>(source: &mut S, (): ()) -> Result<Self, BitError> {
        source.seek_to_bit(source.bit_pos() + 64)?;
        Ok(Self)
    }
}

#[test]
fn absolute_seek_hint_is_a_tail_lower_bound() {
    let mut buffer = BitBuf::new();
    buffer.push(&[0]).unwrap();
    assert_eq!(
        buffer
            .try_pull_with::<Seeking, _>(Layout::default(), ())
            .unwrap_err()
            .kind,
        ErrorKind::Incomplete { needed: Some(7) }
    );
    for count in 0..7 {
        let mut extended = buffer.clone();
        extended.push(&vec![0xff; count]).unwrap();
        assert!(
            extended
                .try_pull_with::<Seeking, _>(Layout::default(), ())
                .unwrap_err()
                .is_incomplete()
        );
    }
    buffer.push(&[0; 7]).unwrap();
    buffer
        .try_pull_with::<Seeking, _>(Layout::default(), ())
        .unwrap();
    assert!(buffer.is_empty());
}

#[cfg(feature = "std")]
#[test]
fn read_exact_bridge_preserves_shortfall_after_a_partial_read() {
    use std::io::Read;
    #[derive(Debug)]
    struct Exact([u8; 4]);
    impl DecodeWith<()> for Exact {
        fn decode_with<S: Source>(source: &mut S, (): ()) -> Result<Self, BitError> {
            let mut bytes = [0; 4];
            source
                .as_read()
                .read_exact(&mut bytes)
                .map_err(BitError::from)
                .map_err(|error| error.in_field("external"))?;
            Ok(Self(bytes))
        }
    }
    let mut buffer = BitBuf::new();
    buffer.push(&[1, 2]).unwrap();
    let error = buffer
        .try_pull_with::<Exact, _>(Layout::default(), ())
        .unwrap_err();
    // The bridge is bytewise, so its *next operation* requires one byte, not two.
    assert_eq!(error.kind, ErrorKind::Incomplete { needed: Some(1) });
    assert_eq!((error.at, error.field), (16, Some("external")));
    assert_eq!(buffer.bit_pos(), 0);
    buffer.push(&[3, 4]).unwrap();
    assert_eq!(
        buffer
            .try_pull_with::<Exact, _>(Layout::default(), ())
            .unwrap()
            .0,
        [1, 2, 3, 4]
    );
}

#[test]
fn logical_region_exhaustion_never_becomes_incomplete() {
    let mut buffer = BitBuf::new();
    buffer.push(&[1]).unwrap();
    let error = buffer
        .try_pull_with::<Region, _>(Layout::default(), ())
        .unwrap_err();
    assert_eq!(
        error.kind,
        ErrorKind::UnexpectedEof {
            needed: 16,
            remaining: 8
        }
    );
    assert_eq!(error.field, Some("inner"));
    buffer.push(&[2, 3]).unwrap();
    assert_eq!(
        buffer
            .try_pull_with::<Region, _>(Layout::default(), ())
            .unwrap_err(),
        error
    );
    assert_eq!(buffer.bit_len(), 24);
}

#[derive(Debug)]
struct CustomIncomplete;
impl DecodeWith<()> for CustomIncomplete {
    fn decode_with<S: Source>(source: &mut S, (): ()) -> Result<Self, BitError> {
        source.read::<u8>()?;
        Err(
            BitError::new(ErrorKind::Incomplete { needed: Some(3) }, source.bit_pos())
                .in_field("custom"),
        )
    }
}

#[test]
fn custom_incomplete_at_eof_is_terminal_with_original_context() {
    let mut buffer = BitBuf::new();
    buffer.push(&[1]).unwrap();
    let error = buffer
        .pull_eof_with::<CustomIncomplete, _>(Layout::default(), ())
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::IncompleteAtEof { needed: Some(3) });
    assert!(!error.is_incomplete());
    assert_eq!((error.at, error.field), (8, Some("custom")));
    assert_eq!(buffer.try_pull::<u8>().unwrap(), 1);
}

// --- decode_prefix_with: the stateless slice form of the same attempt -----------

#[test]
fn prefix_reports_consumed_bits_and_ignores_the_tail() {
    let bytes = [7, 1, 2, 3, 4, 0xee];
    let (packet, bits) = decode_prefix::<Packet>(&bytes, Layout::default(), false).unwrap();
    assert_eq!((packet.tag, packet.value, bits), (7, 0x0102_0304, 40));
    let (_, bits) = decode_prefix::<Packet>(&bytes, Layout::default(), true).unwrap();
    assert_eq!(bits, 40);
}

#[test]
fn prefix_shortage_is_incomplete_unless_eof() {
    let error = decode_prefix::<Packet>(&[7, 1], Layout::default(), false).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Incomplete { needed: Some(3) });
    assert_eq!((error.at, error.field), (8, Some("value")));
    let error = decode_prefix::<Packet>(&[7, 1], Layout::default(), true).unwrap_err();
    assert_eq!(
        error.kind,
        ErrorKind::UnexpectedEof {
            needed: 32,
            remaining: 8
        }
    );
}

#[test]
fn prefix_of_empty_input_is_a_shortage_not_a_special_case() {
    assert_eq!(
        decode_prefix::<Packet>(&[], Layout::default(), false)
            .unwrap_err()
            .kind,
        ErrorKind::Incomplete { needed: Some(1) }
    );
    assert!(matches!(
        decode_prefix::<Packet>(&[], Layout::default(), true)
            .unwrap_err()
            .kind,
        ErrorKind::UnexpectedEof { remaining: 0, .. }
    ));
}

#[derive(Debug)]
struct Nothing;
impl DecodeWith<()> for Nothing {
    fn decode_with<S: Source>(_: &mut S, (): ()) -> Result<Self, BitError> {
        Ok(Self)
    }
}

#[test]
fn prefix_without_progress_is_definitive_even_on_empty_input() {
    for eof in [false, true] {
        for bytes in [&[][..], &[1][..]] {
            let error =
                decode_prefix_with::<Nothing, _>(bytes, Layout::default(), (), eof).unwrap_err();
            assert_eq!(error.kind, ErrorKind::NoProgress);
        }
    }
}

#[test]
fn prefix_keeps_logical_region_overruns_definitive() {
    // A one-byte region holding a u16 overruns its *logical* bound while the slice
    // still has bytes to spare: never `Incomplete`, with or without EOF.
    for eof in [false, true] {
        let error =
            decode_prefix_with::<Region, _>(&[1, 2, 3], Layout::default(), (), eof).unwrap_err();
        assert_eq!(
            error.kind,
            ErrorKind::UnexpectedEof {
                needed: 16,
                remaining: 8
            }
        );
        assert_eq!(error.field, Some("inner"));
    }
    // A region longer than the slice is physical shortage, a retry signal.
    let error =
        decode_prefix_with::<WindowValue, _>(&[1], Layout::default(), 4, false).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Incomplete { needed: Some(3) });
}

#[test]
fn prefix_custom_incomplete_becomes_terminal_only_at_eof() {
    let error = decode_prefix_with::<CustomIncomplete, _>(&[1, 2], Layout::default(), (), false)
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Incomplete { needed: Some(3) });
    let error = decode_prefix_with::<CustomIncomplete, _>(&[1, 2], Layout::default(), (), true)
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::IncompleteAtEof { needed: Some(3) });
}

#[bin(read_only, ctx(count: usize))]
#[derive(Debug, PartialEq, Eq)]
struct Context {
    #[br(count = count)]
    data: Vec<u8>,
}

struct OwnedArg(Vec<u8>); // deliberately neither Copy nor Clone
impl DecodeWith<OwnedArg> for Context {
    fn decode_with<S: Source>(source: &mut S, arg: OwnedArg) -> Result<Self, BitError> {
        Ok(Self {
            data: source.read_bytes(arg.0.len())?,
        })
    }
}

#[test]
fn explicit_layout_supports_read_only_context_and_consuming_args() {
    let mut buffer = BitBuf::new();
    buffer.push(&[1]).unwrap();
    assert!(
        buffer
            .try_pull_with::<Context, _>(Layout::default(), ContextCtx { count: 2 })
            .unwrap_err()
            .is_incomplete()
    );
    buffer.push(&[2]).unwrap();
    let decoded: Context = buffer
        .try_pull_with(Layout::default(), OwnedArg(vec![0; 2]))
        .unwrap();
    assert_eq!(decoded.data, [1, 2]);
    assert!(
        buffer
            .pull_eof_with::<Context, _>(Layout::default(), ContextCtx { count: 2 })
            .unwrap()
            .is_none()
    );
}

#[bin(read_only)]
#[derive(Debug)]
struct Empty {}

#[test]
fn streamed_zero_width_messages_and_elements_are_rejected() {
    let mut buffer = BitBuf::new();
    buffer.push(&[1]).unwrap();
    assert_eq!(
        buffer
            .try_pull_with::<Empty, _>(Layout::default(), ())
            .unwrap_err()
            .kind,
        ErrorKind::NoProgress
    );
    assert_eq!(
        buffer
            .pull_eof_with::<Empty, _>(Layout::default(), ())
            .unwrap_err()
            .kind,
        ErrorKind::NoProgress
    );
    assert_eq!(buffer.bit_len(), 8);
    let mut source = crate::BitReader::new(&[1]);
    assert_eq!(
        Empty::decode_vec(&mut source, usize::MAX).unwrap_err().kind,
        ErrorKind::NoProgress
    );
    assert!(Empty::decode_vec(&mut source, 0).unwrap().is_empty());
}

#[test]
fn rejection_does_not_compact_or_change_a_partial_cursor() {
    let mut buffer = BitBuf::bounded(2);
    buffer.push(&[0xab, 0xcd]).unwrap();
    assert_eq!(buffer.try_pull::<crate::u12>().unwrap().value(), 0xabc);
    let position = buffer.bit_pos();
    let error = buffer.push(&[1, 2]).unwrap_err();
    assert_eq!((error.cap, error.requested), (2, 3));
    assert_eq!(buffer.bit_pos(), position);
    assert_eq!(buffer.bit_len(), 4);
    buffer.push(&[0xef]).unwrap();
    assert_eq!(buffer.try_pull::<crate::u12>().unwrap().value(), 0xdef);
    assert!(buffer.is_empty());
}

#[test]
fn filling_reclaimed_capacity_never_retains_bytes_past_the_bound() {
    for bits in [8_u8, 12] {
        let mut buffer = BitBuf::bounded(4);
        buffer.push(&[1, 2, 3, 4]).unwrap();
        buffer.read_bits(u32::from(bits)).unwrap();
        buffer.push(&[5]).unwrap();
        assert_eq!(buffer.capacity(), Some(4), "consumed {bits} bits");
        assert_eq!(buffer.bit_len(), 40 - usize::from(bits));
        let position = buffer.bit_pos();
        let error = buffer.push(&[6]).unwrap_err();
        assert_eq!((error.cap, error.requested), (4, 5));
        assert_eq!(buffer.bit_pos(), position);
        assert_eq!(buffer.bit_len(), 40 - usize::from(bits));
        buffer.seek_to_bit(0).unwrap();
        assert_eq!(buffer.bit_len(), 32, "consumed {bits} bits");
        buffer.push(&[]).unwrap();
        assert_eq!(buffer.read_bytes(4).unwrap(), [2, 3, 4, 5]);
        assert!(buffer.is_empty());
    }
}

#[test]
fn allocator_slack_cannot_break_the_bound_after_rewinding() {
    for bits in [8, 12] {
        let mut buffer = BitBuf::bounded(4);
        buffer.push(&[0x11, 0x22, 0x33, 0x44]).unwrap();
        buffer.grow(1); // Vec may reserve more than the logical five-byte cap.
        buffer.read_bits(bits).unwrap();
        buffer.push(&[0x55, 0x66]).unwrap();
        let position = buffer.bit_pos();
        let remaining = buffer.bit_len();
        let error = buffer.push(&[0x77]).unwrap_err();
        assert_eq!((error.cap, error.requested), (5, 6));
        assert_eq!((buffer.bit_pos(), buffer.bit_len()), (position, remaining));
        buffer.seek_to_bit(0).unwrap();
        assert_eq!(buffer.bit_len(), 40);
        buffer.push(&[]).unwrap();
        assert_eq!(buffer.try_pull::<u8>().unwrap(), 0x22);
        assert_eq!(buffer.read_bytes(4).unwrap(), [0x33, 0x44, 0x55, 0x66]);
        assert!(buffer.is_empty());
    }
}

#[test]
fn counts_above_host_width_never_wrap_to_shorter_frames() {
    let beyond_host = (usize::MAX as u128) + 1;
    assert_eq!(beyond_host.to_count(), usize::MAX);
    assert_eq!(crate::u127::new(beyond_host).to_count(), usize::MAX);
    assert_eq!((usize::MAX as u128).to_count(), usize::MAX);
    assert_eq!(0u128.to_count(), 0);
}

#[cfg(feature = "std")]
#[test]
fn io_view_roundtrip_preserves_shortfall_and_field() {
    use std::io::Read;
    #[derive(Debug)]
    struct Io;
    impl DecodeWith<()> for Io {
        fn decode_with<S: Source>(source: &mut S, (): ()) -> Result<Self, BitError> {
            let mut bytes = [0; 2];
            source
                .as_read()
                .read_exact(&mut bytes)
                .map_err(BitError::from)
                .map_err(|e| e.in_field("io"))?;
            Ok(Self)
        }
    }
    let mut buffer = BitBuf::new();
    buffer.push(&[1]).unwrap();
    let error = buffer
        .try_pull_with::<Io, _>(Layout::default(), ())
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Incomplete { needed: Some(1) });
    assert_eq!((error.at, error.field), (8, Some("io")));
    let eof = buffer
        .pull_eof_with::<Io, _>(Layout::default(), ())
        .unwrap_err();
    assert_eq!(
        eof.kind,
        ErrorKind::UnexpectedEof {
            needed: 8,
            remaining: 0
        }
    );
    assert_eq!((eof.at, eof.field), (8, Some("io")));
    buffer.push(&[2]).unwrap();
    assert!(buffer.try_pull_with::<Io, _>(Layout::default(), ()).is_ok());
}

#[cfg(feature = "std")]
#[test]
fn stream_reader_keeps_non_eof_io_failures_and_seek_overflow_is_typed() {
    struct Failure(std::io::ErrorKind);
    impl std::io::Read for Failure {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(self.0.into())
        }
    }
    for kind in [
        std::io::ErrorKind::ConnectionReset,
        std::io::ErrorKind::TimedOut,
        std::io::ErrorKind::WouldBlock,
    ] {
        let mut source = crate::StreamBitReader::new(Failure(kind));
        assert_eq!(source.read::<u8>().unwrap_err().kind, ErrorKind::Io(kind));
    }
    let mut source = crate::BufSource::new(&[][..]);
    source.seek_to_bit(usize::MAX).unwrap();
    assert_eq!(
        source.read_bits(1).unwrap_err().kind,
        ErrorKind::PositionOverflow
    );
}
