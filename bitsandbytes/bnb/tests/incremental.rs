//! Integration/property proof across variable magic, versioned records, and packed bits.

mod integration {
    use bnb::{BitBuf, BitError, ErrorKind, Source, bin};

    // Sample representative contents at every shorter length, including bytes that
    // will ultimately produce a hard error. Atomic producers do not inspect them yet.
    fn assert_lower_bound<T: bnb::BitDecode + bnb::BitEncode + core::fmt::Debug>(
        buffer: &BitBuf,
        layout: bnb::Layout,
    ) {
        let error = buffer
            .clone()
            .try_pull_with::<T, _>(layout, ())
            .unwrap_err();
        let ErrorKind::Incomplete {
            needed: Some(needed),
        } = error.kind
        else {
            panic!("fixture must exercise a positive hint: {error}");
        };
        assert!(needed > 0);
        for additional in 0..needed {
            for byte in [0, 0x41, 0xff] {
                let mut extended = buffer.clone();
                extended.push(&vec![byte; additional]).unwrap();
                assert!(
                    extended
                        .try_pull_with::<T, _>(layout, ())
                        .unwrap_err()
                        .is_incomplete(),
                    "{additional} < {needed} bytes, fill {byte:#x}"
                );
            }
        }
    }

    #[test]
    fn scalar_and_bulk_hints_count_physical_bytes_in_both_bit_orders() {
        for bit in [bnb::BitOrder::Msb, bnb::BitOrder::Lsb] {
            for start in 0..8 {
                for length in 1..5 {
                    let layout = bnb::Layout {
                        bit,
                        byte: bnb::ByteOrder::Big,
                    };
                    let mut buffer = BitBuf::new().with_layout(layout);
                    buffer.push(&vec![0; length]).unwrap();
                    buffer.seek_to_bit(start).unwrap();
                    let error = buffer
                        .clone()
                        .try_pull_with::<u64, _>(layout, ())
                        .unwrap_err();
                    let ErrorKind::Incomplete {
                        needed: Some(needed),
                    } = error.kind
                    else {
                        panic!("scalar hint")
                    };
                    assert_eq!(needed, (64 - (length * 8 - start)).div_ceil(8));
                    for count in 0..needed {
                        let mut extended = buffer.clone();
                        extended.push(&vec![0xff; count]).unwrap();
                        assert!(
                            extended
                                .try_pull_with::<u64, _>(layout, ())
                                .unwrap_err()
                                .is_incomplete()
                        );
                    }
                    assert_lower_bound::<EightBytes>(&buffer, layout);
                }
            }
        }
    }

    #[bin(big)]
    #[derive(Debug)]
    struct EightBytes {
        #[br(count = 8)]
        bytes: Vec<u8>,
    }

    #[bin(big)]
    #[derive(Debug)]
    struct TextWire {
        #[brw(count_prefix = u16)]
        bytes: Vec<u8>,
    }

    impl From<&Utf8Body> for TextWire {
        fn from(value: &Utf8Body) -> Self {
            Self {
                bytes: value.text.as_bytes().to_vec(),
            }
        }
    }

    #[bin(try_wire = TextWire)]
    #[derive(Debug)]
    struct Utf8Body {
        text: String,
    }

    impl TryFrom<TextWire> for Utf8Body {
        type Error = std::string::FromUtf8Error;
        fn try_from(value: TextWire) -> Result<Self, Self::Error> {
            Ok(Self {
                text: String::from_utf8(value.bytes)?,
            })
        }
    }

    #[bin(big, magic = 0x4142_4344u32)]
    #[derive(Debug)]
    struct WideMagic {}

    #[bin(big)]
    #[derive(Debug, PartialEq, Eq)]
    enum AlternativeMagic {
        #[bin(magic = b"ABCDE")]
        Long,
        #[bin(magic = b"AX")]
        Short,
        Raw(u8),
    }

    #[test]
    fn hints_do_not_delay_discovery_of_an_earlier_terminal_or_alternative() {
        let mut buffer = BitBuf::new();
        buffer.push(&[0]).unwrap();
        assert_lower_bound::<WideMagic>(&buffer, bnb::Layout::default());
        buffer.push(&[0, 0, 0]).unwrap();
        assert!(matches!(
            buffer.try_pull::<WideMagic>().unwrap_err().kind,
            ErrorKind::BadMagic { .. }
        ));

        let mut buffer = BitBuf::new();
        buffer.push(b"A").unwrap();
        assert_eq!(
            buffer.try_pull::<AlternativeMagic>().unwrap_err().kind,
            ErrorKind::Incomplete { needed: Some(1) }
        );
        buffer.push(b"X").unwrap();
        assert_eq!(
            buffer.try_pull::<AlternativeMagic>().unwrap(),
            AlternativeMagic::Short
        );
        assert_eq!(
            AlternativeMagic::peek_variant(b"AX").unwrap(),
            AlternativeMagicKind::Short
        );
        assert_eq!(
            AlternativeMagic::peek_variant(b"AZ").unwrap(),
            AlternativeMagicKind::Raw
        );
    }

    #[test]
    fn mapped_validation_waits_for_the_whole_wire_value() {
        let mut buffer = BitBuf::new();
        buffer.push(&[0, 4, 0xff]).unwrap();
        assert_lower_bound::<Utf8Body>(&buffer, bnb::Layout::default());
        buffer.push(&[0, 0, 0]).unwrap();
        assert!(matches!(
            buffer.try_pull::<Utf8Body>().unwrap_err().kind,
            ErrorKind::Convert { .. }
        ));
    }

    #[bin(big)]
    #[derive(Debug)]
    struct Restored {
        #[br(restore_position)]
        value: u32,
        tag: u8,
    }

    #[test]
    fn restore_hints_survive_compaction_of_a_previous_message() {
        let mut buffer = BitBuf::bounded(8);
        buffer.push(&[0xaa, 0xbb, 1]).unwrap();
        assert_eq!(buffer.try_pull::<u16>().unwrap(), 0xaabb);
        assert_lower_bound::<Restored>(&buffer, bnb::Layout::default());
        buffer.push(&[2]).unwrap(); // dead prefix exceeds live bytes: push rebases the cursor.
        assert_eq!(buffer.bit_pos(), 0);
        assert_lower_bound::<Restored>(&buffer, bnb::Layout::default());
        buffer.push(&[3, 4]).unwrap();
        let value = buffer.try_pull::<Restored>().unwrap();
        assert_eq!((value.value, value.tag), (0x0102_0304, 1));
        assert_eq!(buffer.bit_len(), 24);
    }

    #[bin(big)]
    #[derive(Debug, PartialEq, Eq)]
    enum LongFirst {
        #[bin(magic = b"AB")]
        Long,
        #[bin(magic = b"A")]
        Short,
        Raw(u8),
    }

    #[bin(big)]
    #[derive(Debug, PartialEq, Eq)]
    enum ShortFirst {
        #[bin(magic = b"A")]
        Short,
        #[bin(magic = b"AB")]
        Long,
    }

    #[test]
    fn partial_magic_preserves_declaration_order_and_finite_fallback() {
        let mut buffer = BitBuf::new();
        buffer.push(b"A").unwrap();
        let error = buffer.try_pull::<LongFirst>().unwrap_err();
        assert_eq!(error.kind, ErrorKind::Incomplete { needed: Some(1) });
        assert_eq!((error.at, error.field), (8, Some("magic")));
        assert_eq!(buffer.bit_len(), 8);
        assert_eq!(
            buffer.pull_eof::<LongFirst>().unwrap(),
            Some(LongFirst::Short)
        );
        buffer.push(b"A").unwrap();
        assert_eq!(buffer.try_pull::<ShortFirst>().unwrap(), ShortFirst::Short);
        buffer.push(b"ABZ").unwrap();
        assert_eq!(buffer.try_pull::<LongFirst>().unwrap(), LongFirst::Long);
        assert_eq!(
            buffer.try_pull::<LongFirst>().unwrap(),
            LongFirst::Raw(b'Z')
        );
        assert!(buffer.is_empty());
        assert_eq!(LongFirst::peek_variant(b"A").unwrap(), LongFirstKind::Short);
    }

    #[bin(big)]
    #[derive(Debug, PartialEq, Eq)]
    enum Signature {
        #[bin(magic = b"HEAD")]
        Head(u16),
        Raw(u8),
    }

    #[test]
    fn every_signature_split_waits_and_definite_mismatch_can_fall_back() {
        for split in 1..6 {
            let mut buffer = BitBuf::new();
            buffer.push(&b"HEAD\x01\x02"[..split]).unwrap();
            assert!(buffer.try_pull::<Signature>().unwrap_err().is_incomplete());
            buffer.push(&b"HEAD\x01\x02"[split..]).unwrap();
            assert_eq!(
                buffer.try_pull::<Signature>().unwrap(),
                Signature::Head(0x0102)
            );
        }
        let mut buffer = BitBuf::new();
        buffer.push(b"X").unwrap();
        assert_eq!(
            buffer.try_pull::<Signature>().unwrap(),
            Signature::Raw(b'X')
        );
        buffer.push(b"H").unwrap();
        assert!(buffer.try_pull::<Signature>().unwrap_err().is_incomplete());
        assert_eq!(
            buffer.pull_eof::<Signature>().unwrap(),
            Some(Signature::Raw(b'H'))
        );
    }

    #[bin(read_only, ctx(version: u8))]
    #[derive(Debug, PartialEq, Eq)]
    struct Versioned {
        #[br(if(version >= 2))]
        extra: Option<u16>,
        tag: u8,
    }

    #[test]
    fn versioned_directional_decode_uses_explicit_layout_and_fresh_context() {
        let mut buffer = BitBuf::new();
        buffer.push(&[1, 2]).unwrap();
        let layout = bnb::Layout {
            bit: bnb::BitOrder::Msb,
            byte: bnb::ByteOrder::Little,
        };
        assert!(
            buffer
                .try_pull_with::<Versioned, _>(layout, VersionedCtx { version: 2 })
                .unwrap_err()
                .is_incomplete()
        );
        buffer.push(&[3, 4]).unwrap();
        assert_eq!(
            buffer
                .try_pull_with::<Versioned, _>(layout, VersionedCtx { version: 2 })
                .unwrap(),
            Versioned {
                extra: Some(0x0201),
                tag: 3
            }
        );
        assert_eq!(
            buffer
                .pull_eof_with::<Versioned, _>(layout, VersionedCtx { version: 1 })
                .unwrap(),
            Some(Versioned {
                extra: None,
                tag: 4
            })
        );
    }

    #[bin(read_only)]
    #[derive(Debug)]
    struct Zero {}
    #[bin(read_only)]
    #[derive(Debug)]
    struct CountedZero {
        #[brw(count_prefix = u128)]
        values: Vec<Zero>,
    }

    #[test]
    fn generated_counted_fields_reject_zero_progress_and_wide_counts() {
        let error = CountedZero::decode_exact(&u128::MAX.to_be_bytes()).unwrap_err();
        assert_eq!(error.kind, ErrorKind::NoProgress);
        assert_eq!(error.field, Some("values"));
        assert!(
            CountedZero::decode_exact(&0u128.to_be_bytes())
                .unwrap()
                .values
                .is_empty()
        );
    }

    #[bin(read_only, ctx(width: u8))]
    #[derive(Debug)]
    struct ContextElement {
        #[br(count = width)]
        bytes: Vec<u8>,
    }
    #[bin(read_only)]
    #[derive(Debug)]
    struct ContextCollection {
        width: u8,
        #[brw(count_prefix = u8)]
        #[br(ctx { width })]
        values: Vec<ContextElement>,
    }

    #[test]
    fn context_counted_elements_also_require_progress() {
        let error = ContextCollection::decode_exact(&[0, 1]).unwrap_err();
        assert_eq!(error.kind, ErrorKind::NoProgress);
        assert_eq!((error.at, error.field), (16, Some("values")));
        let collection = ContextCollection::decode_exact(&[1, 1, 42]).unwrap();
        assert_eq!(collection.values[0].bytes, [42]);
    }

    #[bin(read_only)]
    #[derive(Debug)]
    struct WideCount {
        count: u128,
        #[br(count = count)]
        bytes: Vec<u8>,
    }

    #[bin(read_only)]
    #[derive(Debug)]
    struct WideContextCount {
        count: u128,
        #[br(calc = 1)]
        width: u8,
        #[br(count = count, ctx { width })]
        values: Vec<ContextElement>,
    }

    #[bin(read_only, ctx(count: i64))]
    #[derive(Debug)]
    struct SignedCount {
        #[br(count = count)]
        bytes: Vec<u8>,
    }

    #[test]
    fn explicit_counts_reject_host_width_overflow_and_negative_context() {
        let overflow = (usize::MAX as u128 + 1).to_be_bytes();
        for error in [
            WideCount::decode_exact(&overflow).unwrap_err(),
            WideContextCount::decode_exact(&overflow).unwrap_err(),
        ] {
            assert!(matches!(error.kind, ErrorKind::Convert { .. }));
            assert_eq!(error.at, 128);
            assert!(matches!(error.field, Some("bytes" | "values")));
        }
        let mut wire = 1u128.to_be_bytes().to_vec();
        wire.push(42);
        let direct = WideCount::decode_exact(&wire).unwrap();
        assert_eq!(direct.count, 1);
        assert_eq!(direct.bytes, [42]);
        let contextual = WideContextCount::decode_exact(&wire).unwrap();
        assert_eq!(contextual.count, 1);
        assert_eq!(contextual.width, 1);
        assert_eq!(contextual.values[0].bytes, [42]);
        let mut buffer = BitBuf::new();
        buffer.push(&overflow).unwrap();
        assert!(
            !buffer
                .try_pull_with::<WideCount, _>(bnb::Layout::default(), ())
                .unwrap_err()
                .is_incomplete()
        );
        assert_eq!(buffer.bit_len(), 128);
        let error = buffer
            .try_pull_with::<SignedCount, _>(bnb::Layout::default(), SignedCountCtx { count: -1 })
            .unwrap_err();
        assert!(matches!(error.kind, ErrorKind::Convert { .. }));
        assert_eq!((error.at, error.field), (0, Some("bytes")));
        assert_eq!(
            buffer
                .try_pull_with::<SignedCount, _>(
                    bnb::Layout::default(),
                    SignedCountCtx { count: 0 }
                )
                .unwrap_err()
                .kind,
            ErrorKind::NoProgress
        );
        assert_eq!(
            buffer
                .try_pull_with::<SignedCount, _>(
                    bnb::Layout::default(),
                    SignedCountCtx { count: 1 }
                )
                .unwrap()
                .bytes,
            [0]
        );
    }

    #[bin(read_only)]
    #[derive(Debug)]
    struct WidePointer {
        offset: u128,
        #[br(seek = offset)]
        target: u8,
    }

    #[bin(read_only, ctx(offset: i64))]
    #[derive(Debug)]
    struct SignedPointer {
        #[br(seek = offset)]
        target: u8,
    }

    #[test]
    fn pointer_offsets_never_wrap_to_another_wire_location() {
        let overflow = (usize::MAX as u128 + 1).to_be_bytes();
        let error = WidePointer::peek(&overflow).unwrap_err();
        assert!(matches!(error.kind, ErrorKind::Convert { .. }));
        assert_eq!((error.at, error.field), (128, Some("target")));
        let mut wire = 128u128.to_be_bytes().to_vec();
        wire.push(42);
        let decoded = WidePointer::decode_exact(&wire).unwrap();
        assert_eq!((decoded.offset, decoded.target), (128, 42));
        let mut buffer = BitBuf::new();
        buffer.push(&[42]).unwrap();
        let error = buffer
            .try_pull_with::<SignedPointer, _>(
                bnb::Layout::default(),
                SignedPointerCtx { offset: -1 },
            )
            .unwrap_err();
        assert!(matches!(error.kind, ErrorKind::Convert { .. }));
        assert_eq!((error.at, error.field), (0, Some("target")));
        assert_eq!(buffer.bit_len(), 8);
        assert_eq!(
            buffer
                .try_pull_with::<SignedPointer, _>(
                    bnb::Layout::default(),
                    SignedPointerCtx { offset: 0 }
                )
                .unwrap()
                .target,
            42
        );
    }

    // A downstream element can specialize bulk decode without changing its wire
    // semantics. The generated collection must call that override, not guess its type.
    #[derive(Debug, PartialEq, Eq)]
    struct CustomByte(u8);

    static BULK_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    impl bnb::BitDecode for CustomByte {
        fn bit_decode<S: Source>(source: &mut S) -> Result<Self, BitError> {
            source.read::<u8>().map(Self)
        }

        fn decode_vec<S: Source>(source: &mut S, count: usize) -> Result<Vec<Self>, BitError> {
            BULK_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            source
                .read_bytes(count)
                .map(|bytes| bytes.into_iter().map(Self).collect())
        }
    }

    #[bin(read_only)]
    #[derive(Debug)]
    struct CustomCollection {
        #[brw(count_prefix = u8)]
        values: Vec<CustomByte>,
    }

    #[test]
    fn generated_collection_respects_custom_bulk_decoder() {
        use std::sync::atomic::Ordering;

        let values = CustomCollection::decode_exact(&[2, 42, 43]).unwrap().values;
        assert_eq!(values, [CustomByte(42), CustomByte(43)]);
        assert_eq!(BULK_CALLS.load(Ordering::Relaxed), 1);
        assert!(
            CustomCollection::decode_exact(&[0])
                .unwrap()
                .values
                .is_empty()
        );
        assert_eq!(BULK_CALLS.load(Ordering::Relaxed), 2);
        let mut buffer = BitBuf::new();
        buffer.push(&[2, 42]).unwrap();
        assert!(
            buffer
                .try_pull_with::<CustomCollection, _>(bnb::Layout::default(), ())
                .unwrap_err()
                .is_incomplete()
        );
        assert_eq!(buffer.bit_len(), 16);
        buffer.push(&[43]).unwrap();
        assert_eq!(
            buffer
                .try_pull_with::<CustomCollection, _>(bnb::Layout::default(), ())
                .unwrap()
                .values,
            values
        );
        assert_eq!(BULK_CALLS.load(Ordering::Relaxed), 4);
    }

    #[test]
    fn byte_vectors_at_unaligned_cursors_preserve_both_bit_orders() {
        use bnb::{BitEncode, BitOrder, BitWriter, ByteOrder, Layout};
        #[bin(big)]
        #[derive(Debug, PartialEq, Eq)]
        struct PackedBlob {
            tag: bnb::u4,
            #[brw(count_prefix = u8)]
            body: Vec<u8>,
        }
        let message = PackedBlob {
            tag: bnb::u4::new(0xa),
            body: vec![0x13, 0x57],
        };
        for (bit, golden) in [
            (BitOrder::Msb, [0xa0, 0x21, 0x35, 0x70]),
            (BitOrder::Lsb, [0x2a, 0x30, 0x71, 0x05]),
        ] {
            let layout = Layout {
                bit,
                byte: ByteOrder::Big,
            };
            let mut writer = BitWriter::with_layout(layout);
            message.bit_encode(&mut writer).unwrap();
            assert_eq!(writer.into_bytes(), golden);
            let mut buffer = BitBuf::new();
            for byte in &golden[..3] {
                buffer.push(&[*byte]).unwrap();
                assert!(
                    buffer
                        .try_pull_with::<PackedBlob, _>(layout, ())
                        .unwrap_err()
                        .is_incomplete()
                );
            }
            buffer.push(&golden[3..]).unwrap();
            assert_eq!(
                buffer.try_pull_with::<PackedBlob, _>(layout, ()).unwrap(),
                message
            );
            assert_eq!(buffer.bit_len(), 4);
            assert_eq!(
                buffer
                    .try_pull_with::<bnb::u4, _>(layout, ())
                    .unwrap()
                    .value(),
                0
            );
        }
    }

    #[test]
    fn explicit_lsb_layout_reaches_scalars_and_subbyte_starting_payloads() {
        use bnb::{BitOrder, ByteOrder, Layout};

        #[bin(read_only)]
        #[derive(Debug, PartialEq, Eq)]
        struct EarlyPayload {
            tag: bnb::u4,
            #[br(count = 2)]
            body: Vec<u8>,
        }

        let mut buffer = BitBuf::new();
        let layout = Layout {
            bit: BitOrder::Lsb,
            byte: ByteOrder::Big,
        };
        buffer.push(&[0x12, 0x34]).unwrap();
        assert_eq!(buffer.try_pull_with::<u16, _>(layout, ()).unwrap(), 0x1234);
        for (bit, wire) in [
            (BitOrder::Msb, [0xa1, 0x35, 0x70]),
            (BitOrder::Lsb, [0x3a, 0x71, 0x05]),
        ] {
            buffer.clear();
            buffer.push(&wire).unwrap();
            let message = buffer
                .try_pull_with::<EarlyPayload, _>(
                    Layout {
                        bit,
                        byte: ByteOrder::Big,
                    },
                    (),
                )
                .unwrap();
            assert_eq!(message.tag.value(), 0xa);
            assert_eq!(message.body, [0x13, 0x57]);
            assert_eq!(buffer.bit_len(), 4);
        }
    }

    // A callback's hard error must not become a retry merely because it has EOF shape.
    fn logical_error<S: Source>(source: &mut S) -> Result<u8, BitError> {
        source.read::<u8>()?;
        Err(BitError::new(
            ErrorKind::UnexpectedEof {
                needed: 16,
                remaining: 8,
            },
            source.bit_pos(),
        )
        .in_field("bounded"))
    }
    #[bin(read_only)]
    #[derive(Debug)]
    struct Hard {
        #[allow(dead_code)] // This test decoder deliberately always returns an error.
        #[br(parse_with = logical_error)]
        value: u8,
    }

    #[test]
    fn user_eof_shaped_error_is_not_reclassified() {
        let mut buffer = BitBuf::new();
        buffer.push(&[0, 1, 2]).unwrap();
        let error = buffer
            .try_pull_with::<Hard, _>(bnb::Layout::default(), ())
            .unwrap_err();
        assert!(!error.is_incomplete());
        assert_eq!(error.field, Some("bounded"));
        assert_eq!(buffer.bit_len(), 24);
    }
}

mod property {
    use bnb::{BitBuf, BitEncode, BitOrder, BitWriter, ByteOrder, Layout, bin};
    use proptest::prelude::*;

    #[bin(big)]
    #[derive(Debug, PartialEq, Eq)]
    struct Tlv {
        tag: u8,
        #[brw(count_prefix = u8)]
        body: Vec<u8>,
    }

    proptest! {
        #[test]
        fn arbitrary_partitions_preserve_zero_one_many_messages(
            records in prop::collection::vec((any::<u8>(), prop::collection::vec(any::<u8>(), 0..24)), 0..24),
            chunks in prop::collection::vec(1usize..32, 1..20),
        ) {
            // The oracle writes the wire format directly, independently of the encoder.
            let mut wire = Vec::new();
            for (tag, body) in &records {
                wire.push(*tag);
                wire.push(u8::try_from(body.len()).unwrap());
                wire.extend_from_slice(body);
            }
            let mut buffer = BitBuf::bounded(64);
            let mut offset = 0;
            let mut delivered = 0;
            for chunk in chunks.iter().cycle() {
                if offset == wire.len() { break; }
                let end = (offset + chunk).min(wire.len());
                buffer.push(&wire[offset..end]).unwrap();
                offset = end;
                loop {
                    match buffer.try_pull::<Tlv>() {
                        Ok(message) => {
                            prop_assert_eq!((message.tag, message.body), records[delivered].clone());
                            delivered += 1;
                        }
                        Err(error) => {
                            prop_assert!(error.is_incomplete());
                            break;
                        }
                    }
                }
                buffer.compact();
            }
            prop_assert_eq!(delivered, records.len());
            prop_assert!(buffer.pull_eof::<Tlv>().unwrap().is_none());
        }

        #[test]
        fn packed_54_bit_frames_preserve_all_bits_in_both_orders(
            values in prop::collection::vec(0u64..(1u64 << 54), 4..32),
            chunk_size in 1usize..31,
            lsb in any::<bool>(),
        ) {
            // A multiple of four 54-bit frames ends on a byte boundary.
            let values = &values[..values.len() / 4 * 4];
            let layout = Layout { bit: if lsb { BitOrder::Lsb } else { BitOrder::Msb }, byte: ByteOrder::Big };
            let mut writer = BitWriter::with_layout(layout);
            for value in values { bnb::u54::new(*value).bit_encode(&mut writer).unwrap(); }
            let mut buffer = BitBuf::bounded(40);
            let mut count = 0;
            for chunk in writer.into_bytes().chunks(chunk_size) {
                buffer.push(chunk).unwrap();
                loop {
                    match buffer.try_pull_with::<bnb::u54, _>(layout, ()) {
                        Ok(value) => { prop_assert_eq!(value.value(), values[count]); count += 1; }
                        Err(error) => { prop_assert!(error.is_incomplete()); break; }
                    }
                }
                buffer.compact();
            }
            prop_assert_eq!(count, values.len());
            prop_assert!(buffer.is_empty());
        }
    }
}
