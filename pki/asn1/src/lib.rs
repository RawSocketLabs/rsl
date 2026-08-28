//! Strict ASN.1 Distinguished Encoding Rules (DER) transport.
//!
//! This crate owns tag-length-value mechanics and canonical DER enforcement. Semantic schemas
//! such as X.509 live above it. Decoding borrows the original bytes so a caller can retain the
//! exact signed representation; encoding always emits the distinguished form.
//!
//! ## Standards ownership
//!
//! ITU-T X.690 (02/2021) §§8, 10, and 11 control the identifier, definite-length, primitive
//! contents, and DER canonicalization rules implemented here. See `STANDARDS.md`.
//!
//! This implementation is unaudited and makes no production-security claim.

#![no_std]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

extern crate alloc;

use alloc::{vec, vec::Vec};
use bnb::{BitError, BitReader, BitWriter, Sink, Source};
use core::{cmp::Ordering, fmt, str};

const MAX_NESTING_DEPTH: usize = 32;

/// Result type for strict DER operations.
pub type Result<T> = core::result::Result<T, Error>;

/// ASN.1 identifier class from X.690 §8.1.2.2.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Class {
    /// Universal ASN.1 type.
    Universal,
    /// Application-defined type.
    Application,
    /// Context-specific schema field.
    ContextSpecific,
    /// Private schema field.
    Private,
}

/// A decoded ASN.1 identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Tag {
    /// Identifier class.
    pub class: Class,
    /// Whether the contents contain nested ASN.1 values.
    pub constructed: bool,
    /// Tag number.
    pub number: u32,
}

impl Tag {
    /// Creates a universal primitive tag.
    #[must_use]
    pub const fn universal(number: u32) -> Self {
        Self {
            class: Class::Universal,
            constructed: false,
            number,
        }
    }

    /// Creates a context-specific tag.
    #[must_use]
    pub const fn context(number: u32, constructed: bool) -> Self {
        Self {
            class: Class::ContextSpecific,
            constructed,
            number,
        }
    }

    /// Universal `BOOLEAN`.
    pub const BOOLEAN: Self = Self::universal(1);
    /// Universal `INTEGER`.
    pub const INTEGER: Self = Self::universal(2);
    /// Universal `BIT STRING`.
    pub const BIT_STRING: Self = Self::universal(3);
    /// Universal `OCTET STRING`.
    pub const OCTET_STRING: Self = Self::universal(4);
    /// Universal `NULL`.
    pub const NULL: Self = Self::universal(5);
    /// Universal `OBJECT IDENTIFIER`.
    pub const OBJECT_IDENTIFIER: Self = Self::universal(6);
    /// Universal `UTF8String`.
    pub const UTF8_STRING: Self = Self::universal(12);
    /// Universal constructed `SEQUENCE`.
    pub const SEQUENCE: Self = Self {
        class: Class::Universal,
        constructed: true,
        number: 16,
    };
    /// Universal constructed `SET`.
    pub const SET: Self = Self {
        class: Class::Universal,
        constructed: true,
        number: 17,
    };
    /// Universal `PrintableString`.
    pub const PRINTABLE_STRING: Self = Self::universal(19);
    /// Universal `IA5String`.
    pub const IA5_STRING: Self = Self::universal(22);
    /// Universal `UTCTime`.
    pub const UTC_TIME: Self = Self::universal(23);
    /// Universal `GeneralizedTime`.
    pub const GENERALIZED_TIME: Self = Self::universal(24);
}

/// A strict DER failure with its byte offset in the input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    /// Failure category.
    pub kind: ErrorKind,
    /// Byte offset at which the failure was detected.
    pub offset: usize,
}

impl Error {
    fn new(kind: ErrorKind, offset: usize) -> Self {
        Self { kind, offset }
    }
}

/// Strict DER failure categories.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// The underlying byte source ended early or rejected the operation.
    Source,
    /// Bytes remained after an exact decode.
    TrailingData,
    /// ASN.1 nesting exceeded the implementation's defensive bound.
    NestingTooDeep,
    /// An identifier used a non-minimal or unterminated high-tag-number form.
    InvalidIdentifier,
    /// A tag number exceeded the supported `u32` representation.
    TagOverflow,
    /// DER forbids the BER indefinite-length form.
    IndefiniteLength,
    /// A length used a non-minimal or invalid encoding.
    NonCanonicalLength,
    /// A length or end offset exceeded the target's address space.
    LengthOverflow,
    /// A value had a different tag than its schema requires.
    UnexpectedTag {
        /// Required identifier.
        expected: Tag,
        /// Received identifier.
        actual: Tag,
    },
    /// A universal primitive/constructed bit did not match its ASN.1 type.
    InvalidConstruction,
    /// `BOOLEAN` contents were not the one-byte DER form.
    InvalidBoolean,
    /// `INTEGER` was empty or not minimally encoded.
    InvalidInteger,
    /// `BIT STRING` had an invalid unused-bit count or nonzero padding.
    InvalidBitString,
    /// `NULL` carried contents.
    InvalidNull,
    /// `OBJECT IDENTIFIER` was malformed, non-minimal, or too large.
    InvalidObjectIdentifier,
    /// A text value was not valid for its ASN.1 string type.
    InvalidString,
    /// `SET OF` members were not ordered by their complete DER encodings.
    UnsortedSet,
    /// An encoder received an invalid semantic value.
    InvalidValue,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "DER error {:?} at byte {}",
            self.kind, self.offset
        )
    }
}

impl core::error::Error for Error {}

#[allow(clippy::needless_pass_by_value)]
fn source_error(error: BitError) -> Error {
    Error::new(ErrorKind::Source, error.at / 8)
}

/// One borrowed DER tag-length-value element.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Element<'a> {
    tag: Tag,
    contents: &'a [u8],
    encoded: &'a [u8],
}

impl<'a> Element<'a> {
    /// Decoded identifier.
    #[must_use]
    pub const fn tag(&self) -> Tag {
        self.tag
    }

    /// Contents octets, excluding identifier and length.
    #[must_use]
    pub const fn contents(&self) -> &'a [u8] {
        self.contents
    }

    /// Exact identifier, length, and contents bytes from the input.
    #[must_use]
    pub const fn encoded(&self) -> &'a [u8] {
        self.encoded
    }

    /// Requires `expected` and returns this element unchanged.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::UnexpectedTag`] when the identifier differs.
    pub fn expect(self, expected: Tag) -> Result<Self> {
        if self.tag == expected {
            Ok(self)
        } else {
            Err(Error::new(
                ErrorKind::UnexpectedTag {
                    expected,
                    actual: self.tag,
                },
                0,
            ))
        }
    }

    /// Opens a constructed value as a bounded child decoder.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::InvalidConstruction`] for a primitive value.
    pub fn children(self) -> Result<Decoder<'a>> {
        if !self.tag.constructed {
            return Err(Error::new(ErrorKind::InvalidConstruction, 0));
        }
        Ok(Decoder::new(self.contents))
    }

    /// Decodes a DER boolean.
    ///
    /// # Errors
    ///
    /// The tag or canonical contents are invalid.
    pub fn boolean(self) -> Result<bool> {
        self.expect(Tag::BOOLEAN)?;
        match self.contents {
            [0x00] => Ok(false),
            [0xff] => Ok(true),
            _ => Err(Error::new(ErrorKind::InvalidBoolean, 0)),
        }
    }

    /// Decodes a non-negative integer into `u64`.
    ///
    /// # Errors
    ///
    /// The integer is negative or does not fit in `u64`.
    pub fn unsigned_u64(self) -> Result<u64> {
        let bytes = self.unsigned_bytes()?;
        if bytes.len() > 8 {
            return Err(Error::new(ErrorKind::InvalidValue, 0));
        }
        Ok(bytes
            .iter()
            .fold(0_u64, |value, byte| (value << 8) | u64::from(*byte)))
    }

    /// Borrows a non-negative integer's unsigned magnitude, omitting DER's sign octet.
    ///
    /// # Errors
    ///
    /// The tag is not `INTEGER` or the value is negative.
    pub fn unsigned_bytes(self) -> Result<&'a [u8]> {
        self.expect(Tag::INTEGER)?;
        if self.contents[0] & 0x80 != 0 {
            return Err(Error::new(ErrorKind::InvalidInteger, 0));
        }
        if self.contents.len() > 1 && self.contents[0] == 0 {
            Ok(&self.contents[1..])
        } else {
            Ok(self.contents)
        }
    }

    /// Decodes a bit string.
    ///
    /// # Errors
    ///
    /// The tag or canonical padding is invalid.
    pub fn bit_string(self) -> Result<BitString<'a>> {
        self.expect(Tag::BIT_STRING)?;
        Ok(BitString {
            unused_bits: self.contents[0],
            bytes: &self.contents[1..],
        })
    }

    /// Decodes an object identifier.
    ///
    /// # Errors
    ///
    /// The tag or base-128 subidentifiers are invalid.
    pub fn object_identifier(self) -> Result<ObjectIdentifier> {
        self.expect(Tag::OBJECT_IDENTIFIER)?;
        decode_oid(self.contents)
    }

    /// Decodes UTF-8 text.
    ///
    /// # Errors
    ///
    /// The tag is not `UTF8String` or the contents are malformed UTF-8.
    pub fn utf8_string(self) -> Result<&'a str> {
        self.expect(Tag::UTF8_STRING)?;
        str::from_utf8(self.contents).map_err(|_| Error::new(ErrorKind::InvalidString, 0))
    }

    /// Decodes ASCII text under an expected string tag.
    ///
    /// # Errors
    ///
    /// The tag differs or a byte is not ASCII.
    pub fn ascii_string(self, expected: Tag) -> Result<&'a str> {
        self.expect(expected)?;
        if !self.contents.is_ascii() {
            return Err(Error::new(ErrorKind::InvalidString, 0));
        }
        str::from_utf8(self.contents).map_err(|_| Error::new(ErrorKind::InvalidString, 0))
    }
}

/// A borrowed ASN.1 bit string.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BitString<'a> {
    unused_bits: u8,
    bytes: &'a [u8],
}

impl<'a> BitString<'a> {
    /// Number of unused low bits in the final byte.
    #[must_use]
    pub const fn unused_bits(&self) -> u8 {
        self.unused_bits
    }

    /// Payload octets, excluding the unused-bit count.
    #[must_use]
    pub const fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// Number of meaningful bits.
    #[must_use]
    pub fn bit_len(&self) -> usize {
        self.bytes.len() * 8 - usize::from(self.unused_bits)
    }

    /// Tests a bit using ASN.1's most-significant-bit-first numbering.
    #[must_use]
    pub fn bit(&self, index: usize) -> bool {
        index < self.bit_len() && self.bytes[index / 8] & (0x80 >> (index % 8)) != 0
    }
}

/// An owned ASN.1 object identifier.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ObjectIdentifier(Vec<u64>);

impl ObjectIdentifier {
    /// Builds a semantically valid object identifier from arcs.
    ///
    /// # Errors
    ///
    /// Fewer than two arcs, a first arc above two, or an invalid second arc.
    pub fn from_arcs(arcs: &[u64]) -> Result<Self> {
        if arcs.len() < 2 || arcs[0] > 2 || (arcs[0] < 2 && arcs[1] >= 40) {
            return Err(Error::new(ErrorKind::InvalidObjectIdentifier, 0));
        }
        Ok(Self(arcs.to_vec()))
    }

    /// Borrows the numeric arcs.
    #[must_use]
    pub fn arcs(&self) -> &[u64] {
        &self.0
    }

    /// Compares against a constant arc sequence without allocating another identifier.
    #[must_use]
    pub fn is(&self, arcs: &[u64]) -> bool {
        self.0 == arcs
    }
}

impl fmt::Display for ObjectIdentifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, arc) in self.0.iter().enumerate() {
            if index != 0 {
                formatter.write_str(".")?;
            }
            write!(formatter, "{arc}")?;
        }
        Ok(())
    }
}

/// A borrowed, forward-only decoder over one DER region.
#[derive(Debug)]
pub struct Decoder<'a> {
    input: &'a [u8],
    reader: BitReader<'a>,
    depth: usize,
}

impl<'a> Decoder<'a> {
    /// Starts at the first byte of `input`.
    #[must_use]
    pub fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            reader: BitReader::new(input),
            depth: 0,
        }
    }

    fn nested(input: &'a [u8], depth: usize) -> Result<Self> {
        if depth > MAX_NESTING_DEPTH {
            return Err(Error::new(ErrorKind::NestingTooDeep, 0));
        }
        Ok(Self {
            input,
            reader: BitReader::new(input),
            depth,
        })
    }

    /// Whether this DER region has been consumed completely.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.reader.remaining_bits() == 0
    }

    /// Current byte offset within this region.
    #[must_use]
    pub fn position(&self) -> usize {
        self.reader.bit_pos() / 8
    }

    /// Reads one complete element and validates its local DER canonical form.
    ///
    /// # Errors
    ///
    /// Returns a strict DER error for malformed, truncated, or non-canonical input.
    pub fn read(&mut self) -> Result<Element<'a>> {
        let start = self.position();
        let tag = read_tag(&mut self.reader)?;
        let length = read_length(&mut self.reader)?;
        let contents = self.reader.read_slice(length).map_err(source_error)?;
        let end = self.position();
        let element = Element {
            tag,
            contents,
            encoded: &self.input[start..end],
        };
        validate_element(element, self.depth)?;
        Ok(element)
    }

    /// Requires this region to contain no unread bytes.
    ///
    /// # Errors
    ///
    /// [`ErrorKind::TrailingData`] when any bytes remain.
    pub fn finish(self) -> Result<()> {
        if self.is_finished() {
            Ok(())
        } else {
            Err(Error::new(ErrorKind::TrailingData, self.position()))
        }
    }
}

/// Decodes exactly one complete DER element.
///
/// # Errors
///
/// Malformed input or trailing bytes.
pub fn decode_exact(input: &[u8]) -> Result<Element<'_>> {
    let mut decoder = Decoder::new(input);
    let element = decoder.read()?;
    decoder.finish()?;
    Ok(element)
}

fn read_byte<S: Source>(source: &mut S) -> Result<u8> {
    source.read::<u8>().map_err(source_error)
}

fn read_tag<S: Source>(source: &mut S) -> Result<Tag> {
    let offset = source.bit_pos() / 8;
    let first = read_byte(source)?;
    let class = match first >> 6 {
        0 => Class::Universal,
        1 => Class::Application,
        2 => Class::ContextSpecific,
        _ => Class::Private,
    };
    let constructed = first & 0x20 != 0;
    let low = first & 0x1f;
    let number = if low == 0x1f {
        let mut value = 0_u32;
        let mut first_subidentifier = true;
        loop {
            let byte = read_byte(source)?;
            if first_subidentifier && matches!(byte, 0 | 0x80) {
                return Err(Error::new(ErrorKind::InvalidIdentifier, offset));
            }
            first_subidentifier = false;
            value = value
                .checked_mul(128)
                .and_then(|current| current.checked_add(u32::from(byte & 0x7f)))
                .ok_or_else(|| Error::new(ErrorKind::TagOverflow, offset))?;
            if byte & 0x80 == 0 {
                break;
            }
        }
        if value < 31 {
            return Err(Error::new(ErrorKind::InvalidIdentifier, offset));
        }
        value
    } else {
        u32::from(low)
    };
    Ok(Tag {
        class,
        constructed,
        number,
    })
}

fn read_length<S: Source>(source: &mut S) -> Result<usize> {
    let offset = source.bit_pos() / 8;
    let first = read_byte(source)?;
    if first & 0x80 == 0 {
        return Ok(usize::from(first));
    }
    let octets = usize::from(first & 0x7f);
    if octets == 0 {
        return Err(Error::new(ErrorKind::IndefiniteLength, offset));
    }
    if octets > core::mem::size_of::<usize>() {
        return Err(Error::new(ErrorKind::LengthOverflow, offset));
    }
    let first_length = read_byte(source)?;
    if first_length == 0 {
        return Err(Error::new(ErrorKind::NonCanonicalLength, offset));
    }
    let mut length = usize::from(first_length);
    for _ in 1..octets {
        let byte = read_byte(source)?;
        length = length
            .checked_mul(256)
            .and_then(|current| current.checked_add(usize::from(byte)))
            .ok_or_else(|| Error::new(ErrorKind::LengthOverflow, offset))?;
    }
    if length < 128 {
        return Err(Error::new(ErrorKind::NonCanonicalLength, offset));
    }
    Ok(length)
}

fn validate_element(element: Element<'_>, depth: usize) -> Result<()> {
    if element.tag.constructed {
        if element.tag.class == Class::Universal
            && matches!(element.tag.number, 1..=7 | 9..=10 | 12..=15 | 18..=28 | 30)
        {
            return Err(Error::new(ErrorKind::InvalidConstruction, 0));
        }
        return validate_constructed(element.contents, depth + 1, element.tag == Tag::SET);
    }
    if element.tag.class != Class::Universal {
        return Ok(());
    }
    match element.tag.number {
        0 => Err(Error::new(ErrorKind::InvalidIdentifier, 0)),
        8 | 11 | 16 | 17 => Err(Error::new(ErrorKind::InvalidConstruction, 0)),
        1 => validate_boolean(element.contents),
        2 => validate_integer(element.contents),
        3 => validate_bit_string(element.contents),
        5 if element.contents.is_empty() => Ok(()),
        5 => Err(Error::new(ErrorKind::InvalidNull, 0)),
        6 => decode_oid(element.contents).map(|_| ()),
        12 => str::from_utf8(element.contents)
            .map(|_| ())
            .map_err(|_| Error::new(ErrorKind::InvalidString, 0)),
        19 => validate_printable_string(element.contents),
        22 => {
            if element.contents.is_ascii() {
                Ok(())
            } else {
                Err(Error::new(ErrorKind::InvalidString, 0))
            }
        }
        _ => Ok(()),
    }
}

fn validate_boolean(contents: &[u8]) -> Result<()> {
    if matches!(contents, [0x00 | 0xff]) {
        Ok(())
    } else {
        Err(Error::new(ErrorKind::InvalidBoolean, 0))
    }
}

fn validate_integer(contents: &[u8]) -> Result<()> {
    if contents.is_empty()
        || contents.len() > 1
            && ((contents[0] == 0 && contents[1] & 0x80 == 0)
                || (contents[0] == 0xff && contents[1] & 0x80 != 0))
    {
        Err(Error::new(ErrorKind::InvalidInteger, 0))
    } else {
        Ok(())
    }
}

fn validate_bit_string(contents: &[u8]) -> Result<()> {
    let Some((&unused, bytes)) = contents.split_first() else {
        return Err(Error::new(ErrorKind::InvalidBitString, 0));
    };
    if unused > 7 || bytes.is_empty() && unused != 0 {
        return Err(Error::new(ErrorKind::InvalidBitString, 0));
    }
    if let Some(last) = bytes.last() {
        let mask = if unused == 0 { 0 } else { (1_u8 << unused) - 1 };
        if last & mask != 0 {
            return Err(Error::new(ErrorKind::InvalidBitString, 0));
        }
    }
    Ok(())
}

fn validate_printable_string(contents: &[u8]) -> Result<()> {
    if contents.iter().all(|byte| {
        byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b' ' | b'\'' | b'(' | b')' | b'+' | b',' | b'-' | b'.' | b'/' | b':' | b'=' | b'?'
            )
    }) {
        Ok(())
    } else {
        Err(Error::new(ErrorKind::InvalidString, 0))
    }
}

fn validate_constructed(contents: &[u8], depth: usize, require_order: bool) -> Result<()> {
    let mut decoder = Decoder::nested(contents, depth)?;
    let mut previous: Option<&[u8]> = None;
    while !decoder.is_finished() {
        let element = decoder.read()?;
        if require_order
            && previous.is_some_and(|encoded| encoded.cmp(element.encoded) == Ordering::Greater)
        {
            return Err(Error::new(ErrorKind::UnsortedSet, decoder.position()));
        }
        previous = Some(element.encoded);
    }
    Ok(())
}

fn decode_oid(contents: &[u8]) -> Result<ObjectIdentifier> {
    if contents.is_empty() {
        return Err(Error::new(ErrorKind::InvalidObjectIdentifier, 0));
    }
    let mut subidentifiers = Vec::new();
    let mut value = 0_u64;
    let mut at_start = true;
    for &byte in contents {
        if at_start && byte == 0x80 {
            return Err(Error::new(ErrorKind::InvalidObjectIdentifier, 0));
        }
        at_start = false;
        value = value
            .checked_mul(128)
            .and_then(|current| current.checked_add(u64::from(byte & 0x7f)))
            .ok_or_else(|| Error::new(ErrorKind::InvalidObjectIdentifier, 0))?;
        if byte & 0x80 == 0 {
            subidentifiers.push(value);
            value = 0;
            at_start = true;
        }
    }
    if !at_start {
        return Err(Error::new(ErrorKind::InvalidObjectIdentifier, 0));
    }
    let first = subidentifiers[0];
    let first_arc = if first < 40 {
        0
    } else if first < 80 {
        1
    } else {
        2
    };
    let second_arc = first - first_arc * 40;
    let mut arcs = Vec::with_capacity(subidentifiers.len() + 1);
    arcs.push(first_arc);
    arcs.push(second_arc);
    arcs.extend_from_slice(&subidentifiers[1..]);
    Ok(ObjectIdentifier(arcs))
}

/// Canonical DER encoder backed by `bitsandbytes`' byte sink.
#[derive(Debug, Default)]
pub struct Encoder {
    writer: BitWriter,
}

impl Encoder {
    /// Creates an empty encoder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Consumes the encoder and returns its DER bytes.
    #[must_use]
    pub fn finish(self) -> Vec<u8> {
        self.writer.into_bytes()
    }

    /// Appends one element after validating the supplied identifier and contents.
    ///
    /// # Errors
    ///
    /// Invalid semantic contents, identifier overflow, or sink failure.
    pub fn element(&mut self, tag: Tag, contents: &[u8]) -> Result<()> {
        validate_element(
            Element {
                tag,
                contents,
                encoded: &[],
            },
            0,
        )?;
        write_tag(&mut self.writer, tag)?;
        write_length(&mut self.writer, contents.len())?;
        self.writer.write_bytes(contents).map_err(source_error)
    }

    /// Appends already validated complete DER bytes without rewriting them.
    ///
    /// # Errors
    ///
    /// The bytes are not exactly one strict DER element or the sink fails.
    pub fn encoded(&mut self, encoded: &[u8]) -> Result<()> {
        decode_exact(encoded)?;
        self.writer.write_bytes(encoded).map_err(source_error)
    }

    /// Encodes a constructed sequence from a nested encoder.
    ///
    /// # Errors
    ///
    /// The closure or outer encoding fails.
    pub fn sequence<F>(&mut self, build: F) -> Result<()>
    where
        F: FnOnce(&mut Self) -> Result<()>,
    {
        let mut nested = Self::new();
        build(&mut nested)?;
        self.element(Tag::SEQUENCE, &nested.finish())
    }

    /// Encodes `SET OF` members in DER lexicographic order.
    ///
    /// # Errors
    ///
    /// A member is not strict DER or the sink fails.
    pub fn set_of(&mut self, members: &[Vec<u8>]) -> Result<()> {
        let mut ordered = members.to_vec();
        for member in &ordered {
            decode_exact(member)?;
        }
        ordered.sort();
        let contents: Vec<u8> = ordered.into_iter().flatten().collect();
        self.element(Tag::SET, &contents)
    }

    /// Encodes a boolean, omitting no value.
    ///
    /// # Errors
    ///
    /// The sink fails.
    pub fn boolean(&mut self, value: bool) -> Result<()> {
        self.element(Tag::BOOLEAN, &[if value { 0xff } else { 0x00 }])
    }

    /// Encodes `NULL`.
    ///
    /// # Errors
    ///
    /// The sink fails.
    pub fn null(&mut self) -> Result<()> {
        self.element(Tag::NULL, &[])
    }

    /// Encodes a non-negative integer from its unsigned big-endian magnitude.
    ///
    /// # Errors
    ///
    /// The sink fails.
    pub fn unsigned_integer(&mut self, magnitude: &[u8]) -> Result<()> {
        let first_nonzero = magnitude
            .iter()
            .position(|byte| *byte != 0)
            .unwrap_or(magnitude.len());
        let magnitude = &magnitude[first_nonzero..];
        let contents = if magnitude.is_empty() {
            vec![0]
        } else if magnitude[0] & 0x80 != 0 {
            let mut contents = Vec::with_capacity(magnitude.len() + 1);
            contents.push(0);
            contents.extend_from_slice(magnitude);
            contents
        } else {
            magnitude.to_vec()
        };
        self.element(Tag::INTEGER, &contents)
    }

    /// Encodes an object identifier.
    ///
    /// # Errors
    ///
    /// An arc combination overflows or the sink fails.
    pub fn object_identifier(&mut self, oid: &ObjectIdentifier) -> Result<()> {
        let arcs = oid.arcs();
        let first = arcs[0]
            .checked_mul(40)
            .and_then(|value| value.checked_add(arcs[1]))
            .ok_or_else(|| Error::new(ErrorKind::InvalidObjectIdentifier, 0))?;
        let mut contents = Vec::new();
        encode_subidentifier(first, &mut contents);
        for &arc in &arcs[2..] {
            encode_subidentifier(arc, &mut contents);
        }
        self.element(Tag::OBJECT_IDENTIFIER, &contents)
    }

    /// Encodes a bit string.
    ///
    /// # Errors
    ///
    /// The padding is invalid or the sink fails.
    pub fn bit_string(&mut self, unused_bits: u8, bytes: &[u8]) -> Result<()> {
        let mut contents = Vec::with_capacity(bytes.len() + 1);
        contents.push(unused_bits);
        contents.extend_from_slice(bytes);
        self.element(Tag::BIT_STRING, &contents)
    }

    /// Encodes an octet string.
    ///
    /// # Errors
    ///
    /// The sink fails.
    pub fn octet_string(&mut self, bytes: &[u8]) -> Result<()> {
        self.element(Tag::OCTET_STRING, bytes)
    }
}

fn write_tag(sink: &mut BitWriter, tag: Tag) -> Result<()> {
    let class = match tag.class {
        Class::Universal => 0,
        Class::Application => 0x40,
        Class::ContextSpecific => 0x80,
        Class::Private => 0xc0,
    };
    let construction = if tag.constructed { 0x20 } else { 0 };
    if tag.number < 31 {
        return sink
            .write(
                class
                    | construction
                    | u8::try_from(tag.number).expect("short-form tag number fits in u8"),
            )
            .map_err(source_error);
    }
    sink.write(class | construction | 0x1f)
        .map_err(source_error)?;
    let mut encoded = Vec::new();
    encode_subidentifier(u64::from(tag.number), &mut encoded);
    sink.write_bytes(&encoded).map_err(source_error)
}

fn write_length(sink: &mut BitWriter, length: usize) -> Result<()> {
    if length < 128 {
        return sink
            .write(u8::try_from(length).expect("short-form length fits in u8"))
            .map_err(source_error);
    }
    let bytes = length.to_be_bytes();
    let first = bytes
        .iter()
        .position(|byte| *byte != 0)
        .unwrap_or(bytes.len() - 1);
    let significant = &bytes[first..];
    sink.write(
        0x80 | u8::try_from(significant.len()).expect("usize length width fits in one octet"),
    )
    .map_err(source_error)?;
    sink.write_bytes(significant).map_err(source_error)
}

fn encode_subidentifier(mut value: u64, output: &mut Vec<u8>) {
    let mut bytes = [0_u8; 10];
    let mut index = bytes.len() - 1;
    bytes[index] = (value & 0x7f) as u8;
    value >>= 7;
    while value != 0 {
        index -= 1;
        bytes[index] = ((value & 0x7f) as u8) | 0x80;
        value >>= 7;
    }
    output.extend_from_slice(&bytes[index..]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_derived_sequence_round_trip_and_exact_span() {
        let der = [0x30, 0x06, 0x02, 0x01, 0x2a, 0x01, 0x01, 0xff];
        let sequence = decode_exact(&der).unwrap();
        assert_eq!(sequence.encoded(), der);
        let mut children = sequence.children().unwrap();
        assert_eq!(children.read().unwrap().unsigned_u64().unwrap(), 42);
        assert!(children.read().unwrap().boolean().unwrap());
        children.finish().unwrap();

        let mut encoded = Encoder::new();
        encoded
            .sequence(|sequence| {
                sequence.unsigned_integer(&[42])?;
                sequence.boolean(true)
            })
            .unwrap();
        assert_eq!(encoded.finish(), der);
    }

    #[test]
    fn standard_derived_oid_round_trip() {
        let oid = ObjectIdentifier::from_arcs(&[1, 2, 840, 113_549, 1, 1, 1]).unwrap();
        let mut encoder = Encoder::new();
        encoder.object_identifier(&oid).unwrap();
        let der = encoder.finish();
        assert_eq!(
            der,
            [0x06, 0x09, 0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 1, 1, 1]
        );
        assert_eq!(
            decode_exact(&der).unwrap().object_identifier().unwrap(),
            oid
        );
    }

    #[test]
    fn negative_noncanonical_lengths_identifiers_and_integers() {
        assert_eq!(
            decode_exact(&[0x04, 0x81, 0x01, 0]).unwrap_err().kind,
            ErrorKind::NonCanonicalLength
        );
        assert_eq!(
            decode_exact(&[0x04, 0x80, 0, 0]).unwrap_err().kind,
            ErrorKind::IndefiniteLength
        );
        assert_eq!(
            decode_exact(&[0x1f, 0x1e, 0]).unwrap_err().kind,
            ErrorKind::InvalidIdentifier
        );
        assert_eq!(
            decode_exact(&[0x02, 0x02, 0, 1]).unwrap_err().kind,
            ErrorKind::InvalidInteger
        );
        assert_eq!(
            decode_exact(&[0x30, 0x03, 0x04, 0x81, 0]).unwrap_err().kind,
            ErrorKind::NonCanonicalLength
        );
    }

    #[test]
    fn negative_boolean_bit_string_oid_and_set_order() {
        assert_eq!(
            decode_exact(&[0x01, 1, 1]).unwrap_err().kind,
            ErrorKind::InvalidBoolean
        );
        assert_eq!(
            decode_exact(&[0x03, 2, 1, 1]).unwrap_err().kind,
            ErrorKind::InvalidBitString
        );
        assert_eq!(
            decode_exact(&[0x06, 2, 0x80, 0]).unwrap_err().kind,
            ErrorKind::InvalidObjectIdentifier
        );
        assert_eq!(
            decode_exact(&[0x31, 6, 0x02, 1, 2, 0x02, 1, 1])
                .unwrap_err()
                .kind,
            ErrorKind::UnsortedSet
        );
    }

    #[test]
    fn hostile_length_fails_without_allocating_from_the_claim() {
        let mut der = vec![0x04, 0x88];
        der.extend_from_slice(&u64::MAX.to_be_bytes());
        assert!(matches!(
            decode_exact(&der).unwrap_err().kind,
            ErrorKind::Source | ErrorKind::LengthOverflow
        ));
    }

    #[test]
    fn negative_nesting_depth_is_bounded() {
        let mut der = vec![0x05, 0x00];
        for _ in 0..=MAX_NESTING_DEPTH {
            let mut outer = vec![0x30, u8::try_from(der.len()).unwrap()];
            outer.extend_from_slice(&der);
            der = outer;
        }
        assert_eq!(
            decode_exact(&der).unwrap_err().kind,
            ErrorKind::NestingTooDeep
        );
    }
}
