//! Ready-made field codecs for [`parse_with`]/[`write_with`] — LEB128 varints,
//! NUL-terminated C strings, and length-prefixed strings.
//!
//! Reference them by path from a [`#[bin]`](macro@crate::bin) field instead of
//! hand-rolling the read/write pair:
//!
//! | codec | wire form | field type | attributes |
//! |---|---|---|---|
//! | [`leb128`] | 7-bit groups, high-bit continuation | `u8`…`u128` | `parse_with = bnb::codecs::leb128::parse`, `write_with = bnb::codecs::leb128::write` |
//! | [`cstring`] | bytes, `0x00` terminator | `Vec<u8>` / `String` | `parse_with = bnb::codecs::cstring::parse` (or `parse_utf8`), `write_with = …::write` (or `write_utf8`) |
//! | [`prefixed`] | length prefix, then UTF-8 bytes | `String` | `parse_with = bnb::codecs::prefixed::parse_string::<_, u16>`, `write_with = …::write_string::<_, u16>` |
//!
//! Every codec follows the crate's dual-use doctrine: **parse rejects only the
//! unrepresentable** (an overflowing varint, invalid UTF-8 destined for a `String`) and
//! **write refuses only what could not round-trip** (an embedded NUL in a C string, a
//! length exceeding its prefix). Nothing here validates *meaning* — that stays with
//! `validate` at construction.
//!
//! The fn contract (what a hand-rolled codec must also satisfy): a parse fn is
//! `fn(&mut impl Source) -> Result<T, BitError>`, a write fn is
//! `fn(&T, &mut impl Sink) -> Result<(), BitError>` — see the
//! [`guide::directives`](crate::guide::directives) `parse_with` section for rolling your
//! own.
//!
//! Using the same codec on many fields? Wrap it **once** as a per-type newtype —
//! `#[bin(codec = bnb::codecs::leb128)] struct Varint(pub u64);` — and use the type as
//! a plain field everywhere (see the guide's "Per-type codecs" section).
//!
//! Naming note: this is **not** the `codec` module — that one (under the `tokio`
//! feature) is the async `Decoder`/`Encoder` adapter for framed transports; *this*
//! module is the library of ready-made *field* codecs.
//!
//! [`parse_with`]: crate::guide::directives
//! [`write_with`]: crate::guide::directives

pub use crate::bitstream::CountPrefix;

/// Unsigned LEB128 (`varint`) — 7 payload bits per byte, low group first, high bit set
/// while more bytes follow. The wire format of protobuf `varint`, WebAssembly, DWARF.
///
/// Generic over the field's integer width via [`Varint`]: the declared field type pins
/// the width, so the same `parse`/`write` pair serves `u16` and `u64` fields alike.
/// Decoding is **bounded and overflow-checked** — a value too large for the field or a
/// continuation run longer than the width allows is a clean [`BitError`], never a panic
/// or a silent wrap. Non-minimal encodings (e.g. `0x80 0x00` for zero) are accepted:
/// permissive parse, canonical (minimal) write.
///
/// ```
/// use bnb::bin;
///
/// #[bin(big)]
/// #[derive(Debug, PartialEq)]
/// struct Record {
///     #[br(parse_with = bnb::codecs::leb128::parse)]
///     #[bw(write_with = bnb::codecs::leb128::write)]
///     length: u32, // width inferred from the field type
///     #[br(parse_with = bnb::codecs::leb128::parse)]
///     #[bw(write_with = bnb::codecs::leb128::write)]
///     timestamp: u64,
/// }
///
/// let r = Record { length: 300, timestamp: 1 };
/// let bytes = r.to_bytes().unwrap();
/// assert_eq!(bytes, [0xAC, 0x02, 0x01]); // 300 = 0b10_0101100 → AC 02; 1 → 01
/// assert_eq!(Record::decode_exact(&bytes).unwrap(), r);
/// ```
///
/// [`BitError`]: crate::BitError
pub mod leb128 {
    use crate::bitstream::{BitError, Sink, Source, sealed};
    use crate::field::Bits;
    use alloc::format;

    /// The integer widths readable/writable as LEB128 — `u8`, `u16`, `u32`, `u64`,
    /// `u128`. Sealed: LEB128 is byte-granular, so the primitive widths are the whole
    /// set (a `uN` field wants the next wider primitive).
    #[diagnostic::on_unimplemented(
        message = "`{Self}` cannot be read/written as a LEB128 varint",
        note = "supported widths: u8, u16, u32, u64, u128 — declare a `uN` field as the next wider primitive",
        note = "this trait is sealed — the supported widths are built in"
    )]
    pub trait Varint: Bits + sealed::Sealed {}

    impl Varint for u8 {}
    impl Varint for u16 {}
    impl Varint for u32 {}
    impl Varint for u64 {}
    impl Varint for u128 {}

    /// Reads one LEB128 value; the target width comes from the field's declared type.
    ///
    /// # Errors
    /// [`Convert`](crate::bitstream::ErrorKind::Convert) when the value overflows the
    /// width or the continuation run exceeds the width's maximum byte count;
    /// [`UnexpectedEof`](crate::bitstream::ErrorKind::UnexpectedEof) when the input ends
    /// mid-varint.
    pub fn parse<S: Source, T: Varint>(r: &mut S) -> Result<T, BitError> {
        let start = r.bit_pos();
        // ceil(BITS / 7): 2 / 3 / 5 / 10 / 19 bytes for u8 / u16 / u32 / u64 / u128.
        let max_bytes = T::BITS.div_ceil(7);
        let mut value: u128 = 0;
        let mut shift: u32 = 0;
        for _ in 0..max_bytes {
            let byte: u8 = r.read()?;
            let group = u128::from(byte & 0x7F);
            // The loop bound keeps `shift ≤ 7·(max_bytes−1) < T::BITS`, so `T::BITS −
            // shift` never underflows; when the group carries bits past the width,
            // reject instead of wrapping (the hand-rolled version this replaces shifted
            // unbounded — a debug panic on hostile input).
            if shift + 7 > T::BITS && (group >> (T::BITS - shift)) != 0 {
                return Err(BitError::convert(
                    format!("LEB128 value overflows u{}", T::BITS),
                    start,
                ));
            }
            value |= group << shift;
            if byte & 0x80 == 0 {
                return Ok(T::from_bits(value));
            }
            shift += 7;
        }
        Err(BitError::convert(
            format!(
                "unterminated LEB128: no final byte within {max_bytes} bytes (u{})",
                T::BITS
            ),
            start,
        ))
    }

    /// Writes one LEB128 value in canonical (minimal) form.
    ///
    /// # Errors
    /// Only what the underlying [`Sink`] reports (e.g. a bounded buffer running out).
    pub fn write<K: Sink, T: Varint>(v: &T, w: &mut K) -> Result<(), BitError> {
        let mut value = v.into_bits();
        loop {
            let mut byte = (value & 0x7F) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            w.write(byte)?;
            if value == 0 {
                return Ok(());
            }
        }
    }
}

/// NUL-terminated C strings — bytes until (and consuming) a `0x00` terminator.
///
/// Two forms: raw bytes (`parse`/`write` over `Vec<u8>` — permissive, pairs with
/// [`#[try_str]`](macro@crate::bin) for display) and UTF-8 (`parse_utf8`/`write_utf8`
/// over `String` — decode errors on invalid UTF-8, which a `String` physically cannot
/// hold). The terminator is excluded from the value; write appends it, and **errors on
/// an embedded NUL** — the wire image would decode back truncated, so it cannot
/// round-trip.
///
/// ```
/// use bnb::bin;
///
/// #[bin(big)]
/// #[derive(Debug, PartialEq)]
/// struct Entry {
///     id: u8,
///     #[br(parse_with = bnb::codecs::cstring::parse)]
///     #[bw(write_with = bnb::codecs::cstring::write)]
///     #[try_str]
///     name: Vec<u8>,
/// }
///
/// let e = Entry { id: 7, name: b"alpha".to_vec() };
/// let bytes = e.to_bytes().unwrap();
/// assert_eq!(bytes, [7, b'a', b'l', b'p', b'h', b'a', 0]);
/// assert_eq!(Entry::decode_exact(&bytes).unwrap(), e);
/// ```
pub mod cstring {
    use crate::bitstream::{BitError, Sink, Source};
    use alloc::format;
    use alloc::string::String;
    use alloc::vec::Vec;

    /// Reads bytes until a `0x00` terminator (consumed, excluded from the value).
    /// Permissive: any byte sequence is accepted.
    ///
    /// # Errors
    /// [`UnexpectedEof`](crate::bitstream::ErrorKind::UnexpectedEof) when the input ends
    /// before a terminator.
    pub fn parse<S: Source>(r: &mut S) -> Result<Vec<u8>, BitError> {
        let mut v = Vec::new();
        loop {
            let b: u8 = r.read()?;
            if b == 0 {
                return Ok(v);
            }
            v.push(b);
        }
    }

    /// Writes the bytes followed by the `0x00` terminator.
    ///
    /// # Errors
    /// [`Convert`](crate::bitstream::ErrorKind::Convert) on an embedded NUL — the
    /// decoded value would stop there, so the wire image could not round-trip.
    pub fn write<K: Sink>(v: &[u8], w: &mut K) -> Result<(), BitError> {
        if let Some(i) = v.iter().position(|&b| b == 0) {
            return Err(BitError::convert(
                format!("embedded NUL at byte {i}: a NUL-terminated string cannot represent it"),
                w.bit_pos(),
            ));
        }
        for &b in v {
            w.write(b)?;
        }
        w.write(0u8)
    }

    /// [`parse`], then UTF-8-validates into a `String`.
    ///
    /// # Errors
    /// As [`parse`], plus [`Convert`](crate::bitstream::ErrorKind::Convert) on invalid
    /// UTF-8 (a `String` cannot represent it; keep a `Vec<u8>` field + [`parse`] to
    /// preserve arbitrary bytes).
    pub fn parse_utf8<S: Source>(r: &mut S) -> Result<String, BitError> {
        let start = r.bit_pos();
        let bytes = parse(r)?;
        String::from_utf8(bytes)
            .map_err(|e| BitError::convert(format!("invalid UTF-8 in string field: {e}"), start))
    }

    /// Writes the string's UTF-8 bytes followed by the terminator (via [`write`], so an
    /// embedded `'\0'` is rejected the same way).
    ///
    /// # Errors
    /// As [`write`].
    pub fn write_utf8<K: Sink>(s: &str, w: &mut K) -> Result<(), BitError> {
        write(s.as_bytes(), w)
    }
}

/// Length-prefixed UTF-8 strings — a [`CountPrefix`](super::CountPrefix) integer
/// counting the **bytes**, then that many bytes of UTF-8.
///
/// Generic over the prefix type: any type the
/// [`#[brw(count_prefix = …)]`](macro@crate::bin) directive accepts, including the
/// arbitrary-width `uN`s (a `u12` prefix is 12 bits on the wire). Pick it with a
/// turbofish — the cursor parameter stays inferred:
///
/// ```
/// use bnb::bin;
///
/// #[bin(big)]
/// #[derive(Debug, PartialEq)]
/// struct Label {
///     #[br(parse_with = bnb::codecs::prefixed::parse_string::<_, u16>)]
///     #[bw(write_with = bnb::codecs::prefixed::write_string::<_, u16>)]
///     title: String,
/// }
///
/// let l = Label { title: "hi".into() };
/// let bytes = l.to_bytes().unwrap();
/// assert_eq!(bytes, [0x00, 0x02, b'h', b'i']); // u16 byte length, then UTF-8
/// assert_eq!(Label::decode_exact(&bytes).unwrap(), l);
/// ```
///
/// For length-prefixed **bytes** (`Vec<u8>`), use the
/// [`#[brw(count_prefix = …)]`](macro@crate::bin) directive instead — same wire form,
/// one attribute. Write is **checked**: a string longer than the prefix's range is a
/// [`BitError`](crate::BitError), never a wrapped length. (On 32-bit targets a `u64`/
/// `u128` prefix narrows through `usize` on read — see
/// [`CountPrefix::to_count`](super::CountPrefix::to_count).)
pub mod prefixed {
    use crate::bitstream::{BitError, CountPrefix, Sink, Source};
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    /// Reads a `P` byte-length prefix, then that many bytes, UTF-8-validated.
    ///
    /// No pre-allocation from the untrusted length — bytes are pushed as read, bounded
    /// by the input, so a hostile huge prefix is a fast
    /// [`UnexpectedEof`](crate::bitstream::ErrorKind::UnexpectedEof), not an allocation.
    ///
    /// # Errors
    /// `UnexpectedEof` when the input is shorter than the prefix promises;
    /// [`Convert`](crate::bitstream::ErrorKind::Convert) on invalid UTF-8.
    pub fn parse_string<S: Source, P: CountPrefix>(r: &mut S) -> Result<String, BitError> {
        let start = r.bit_pos();
        let n = r.read::<P>()?.to_count();
        let mut bytes = Vec::new();
        for _ in 0..n {
            bytes.push(r.read::<u8>()?);
        }
        String::from_utf8(bytes)
            .map_err(|e| BitError::convert(format!("invalid UTF-8 in string field: {e}"), start))
    }

    /// Writes the `P` byte-length prefix, then the string's UTF-8 bytes.
    ///
    /// # Errors
    /// [`Convert`](crate::bitstream::ErrorKind::Convert) when the byte length exceeds
    /// the prefix's range (checked — never a silently wrapped length).
    pub fn write_string<K: Sink, P: CountPrefix>(s: &str, w: &mut K) -> Result<(), BitError> {
        let prefix =
            P::try_from_len(s.len()).map_err(|e| BitError::convert(e.to_string(), w.bit_pos()))?;
        w.write(prefix)?;
        for &b in s.as_bytes() {
            w.write(b)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod unit {
    use super::*;
    use crate::bitstream::{BitReader, BitWriter, ErrorKind};
    use crate::u12;
    use alloc::vec;
    use alloc::vec::Vec;

    fn encode_with<T: ?Sized>(
        f: impl Fn(&T, &mut BitWriter) -> Result<(), crate::BitError>,
        v: &T,
    ) -> Vec<u8> {
        let mut w = BitWriter::new();
        f(v, &mut w).unwrap();
        w.into_bytes()
    }

    // ——— leb128 ———

    fn leb_round_trip<T: leb128::Varint + PartialEq + core::fmt::Debug>(v: T) {
        let mut w = BitWriter::new();
        leb128::write(&v, &mut w).unwrap();
        let bytes = w.into_bytes();
        let mut r = BitReader::new(&bytes);
        assert_eq!(leb128::parse::<_, T>(&mut r).unwrap(), v);
        assert_eq!(r.remaining_bits(), 0, "consumed exactly the varint");
    }

    #[test]
    fn leb128_round_trips_per_width() {
        for v in [0u8, 1, 127, 128, u8::MAX] {
            leb_round_trip(v);
        }
        for v in [0u16, 127, 128, 0x3FFF, u16::MAX] {
            leb_round_trip(v);
        }
        for v in [0u32, 300, u32::MAX] {
            leb_round_trip(v);
        }
        for v in [0u64, 300, 1_000_000, u64::MAX] {
            leb_round_trip(v);
        }
        for v in [0u128, u128::from(u64::MAX) + 1, u128::MAX] {
            leb_round_trip(v);
        }
    }

    #[test]
    fn leb128_golden_bytes() {
        assert_eq!(encode_with(leb128::write, &300u64), [0xAC, 0x02]);
        assert_eq!(encode_with(leb128::write, &1u32), [0x01]);
    }

    #[test]
    fn leb128_accepts_non_minimal() {
        // 0x80 0x00 is zero with a redundant continuation — permissive parse takes it.
        let mut r = BitReader::new(&[0x80, 0x00]);
        assert_eq!(leb128::parse::<_, u8>(&mut r).unwrap(), 0);
    }

    #[test]
    fn leb128_overflow_is_an_error_not_a_panic() {
        // u8: second byte may only carry one bit. 0xFF 0x01 = 255 fits; 0xFF 0x02 doesn't.
        let mut r = BitReader::new(&[0xFF, 0x01]);
        assert_eq!(leb128::parse::<_, u8>(&mut r).unwrap(), 255);
        let mut r = BitReader::new(&[0xFF, 0x02]);
        let err = leb128::parse::<_, u8>(&mut r).unwrap_err();
        assert!(
            matches!(&err.kind, ErrorKind::Convert { message } if message.contains("overflows u8")),
            "got {err:?}"
        );
        assert_eq!(err.at, 0, "error points at the varint's start");

        // u32: the 5th byte may carry 4 bits; 0x1F sets bit 33 → overflow.
        let mut r = BitReader::new(&[0xFF, 0xFF, 0xFF, 0xFF, 0x1F]);
        let err = leb128::parse::<_, u32>(&mut r).unwrap_err();
        assert!(
            matches!(&err.kind, ErrorKind::Convert { message } if message.contains("overflows u32"))
        );

        // u128: 19th byte may carry 2 bits (18·7 = 126); 0x03 fits, 0x04 overflows.
        let mut ok = vec![0xFF; 18];
        ok.push(0x03);
        let mut r = BitReader::new(&ok);
        assert_eq!(leb128::parse::<_, u128>(&mut r).unwrap(), u128::MAX);
        let mut over = vec![0xFF; 18];
        over.push(0x04);
        let mut r = BitReader::new(&over);
        assert!(leb128::parse::<_, u128>(&mut r).is_err());
    }

    #[test]
    fn leb128_hostile_continuation_run_is_bounded() {
        // 11 continuation bytes against a u64 (max 10): the old hand-rolled loop shifted
        // past 63 and panicked in debug — the shipped codec errors cleanly.
        let bytes = [0x80u8; 11];
        let mut r = BitReader::new(&bytes);
        let err = leb128::parse::<_, u64>(&mut r).unwrap_err();
        assert!(
            matches!(&err.kind, ErrorKind::Convert { message } if message.contains("unterminated")),
            "got {err:?}"
        );
    }

    #[test]
    fn leb128_eof_mid_varint() {
        let mut r = BitReader::new(&[0x80, 0x80]);
        let err = leb128::parse::<_, u64>(&mut r).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::UnexpectedEof { .. }));
    }

    // ——— cstring ———

    #[test]
    fn cstring_round_trips() {
        for s in [&b""[..], b"a", b"alpha", &[0xFF, 0xFE]] {
            let bytes = encode_with(cstring::write, s);
            assert_eq!(bytes.last(), Some(&0));
            let mut r = BitReader::new(&bytes);
            assert_eq!(cstring::parse(&mut r).unwrap(), s);
        }
    }

    #[test]
    fn cstring_eof_before_terminator() {
        let mut r = BitReader::new(b"hi");
        let err = cstring::parse(&mut r).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::UnexpectedEof { .. }));
    }

    #[test]
    fn cstring_write_rejects_embedded_nul() {
        let mut w = BitWriter::new();
        let err = cstring::write(&[1, 0, 2], &mut w).unwrap_err();
        assert!(
            matches!(&err.kind, ErrorKind::Convert { message } if message.contains("byte 1")),
            "got {err:?}"
        );
        // The str form delegates, so it rejects the same way.
        let mut w = BitWriter::new();
        assert!(cstring::write_utf8("a\0b", &mut w).is_err());
    }

    #[test]
    fn cstring_utf8_forms() {
        let bytes = encode_with(cstring::write_utf8, "héllo");
        let mut r = BitReader::new(&bytes);
        assert_eq!(cstring::parse_utf8(&mut r).unwrap(), "héllo");

        let mut r = BitReader::new(&[0xFF, 0x00]);
        let err = cstring::parse_utf8(&mut r).unwrap_err();
        assert!(matches!(&err.kind, ErrorKind::Convert { message } if message.contains("UTF-8")));
    }

    // ——— prefixed ———

    fn prefixed_round_trip<P: CountPrefix>(s: &str) {
        let mut w = BitWriter::new();
        prefixed::write_string::<_, P>(s, &mut w).unwrap();
        let bytes = w.into_bytes();
        let mut r = BitReader::new(&bytes);
        assert_eq!(prefixed::parse_string::<_, P>(&mut r).unwrap(), s);
    }

    #[test]
    fn prefixed_round_trips_across_prefix_types() {
        for s in ["", "x", "hello", "héllo wörld"] {
            prefixed_round_trip::<u8>(s);
            prefixed_round_trip::<u16>(s);
            prefixed_round_trip::<u32>(s);
            prefixed_round_trip::<u12>(s); // a uN prefix: 12 bits on the wire
        }
    }

    #[test]
    fn prefixed_u12_is_bit_native() {
        let mut w = BitWriter::new();
        prefixed::write_string::<_, u12>("hi", &mut w).unwrap();
        // 12-bit length (2) + 'h' + 'i' = 12 + 16 bits = 28 bits → 4 bytes padded.
        let bytes = w.into_bytes();
        assert_eq!(bytes[0], 0x00); // high 8 of the 12-bit length
        assert_eq!(bytes[1] >> 4, 0x2); // low 4 of the length
    }

    #[test]
    fn prefixed_write_overflow_is_checked() {
        let long = "x".repeat(256);
        let mut w = BitWriter::new();
        let err = prefixed::write_string::<_, u8>(&long, &mut w).unwrap_err();
        assert!(
            matches!(&err.kind, ErrorKind::Convert { message } if message.contains("256")),
            "got {err:?}"
        );
        let long = "x".repeat(4096);
        let mut w = BitWriter::new();
        assert!(prefixed::write_string::<_, u12>(&long, &mut w).is_err());
    }

    #[test]
    fn prefixed_hostile_length_is_eof_not_alloc() {
        // Prefix promises u32::MAX bytes; only 3 follow. Push-per-byte means a fast EOF.
        let mut wire = vec![0xFF, 0xFF, 0xFF, 0xFF];
        wire.extend_from_slice(b"abc");
        let mut r = BitReader::new(&wire);
        let err = prefixed::parse_string::<_, u32>(&mut r).unwrap_err();
        assert!(matches!(err.kind, ErrorKind::UnexpectedEof { .. }));
    }

    #[test]
    fn prefixed_invalid_utf8_is_convert() {
        let wire = [0x02, 0xFF, 0xFE];
        let mut r = BitReader::new(&wire);
        let err = prefixed::parse_string::<_, u8>(&mut r).unwrap_err();
        assert!(matches!(&err.kind, ErrorKind::Convert { message } if message.contains("UTF-8")));
    }

    #[test]
    fn error_display_texts() {
        let mut w = BitWriter::new();
        let e = prefixed::write_string::<_, u8>(&"x".repeat(300), &mut w).unwrap_err();
        assert_eq!(
            e.to_string(),
            "conversion failed: value 300 does not fit in 8 bits at bit 0"
        );
    }
}
