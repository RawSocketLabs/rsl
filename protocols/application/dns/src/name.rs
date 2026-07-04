//! Domain names and label parsing, including compression-pointer following on decode
//! (RFC 1035 §3.1, §4.1.4).

use bnb::bin;
use bnb::bitstream::{BitError, Sink, Source};
use std::collections::HashMap;

/// The RFC 1035 §3.1 hard cap on a domain name's total on-wire length (the label bytes plus
/// their length octets plus the root terminator). Enforcing it on decode bounds the
/// compression-amplification a hostile packet can achieve — pointers may chain across the
/// whole message, but one resolved `Name` can never exceed 255 bytes of labels.
const MAX_NAME_LEN: usize = 255;

/// The maximum number of compression-pointer jumps to follow before declaring a loop.
/// RFC 1035 allows arbitrarily-chained pointers, but a well-formed name resolves in far
/// fewer hops than this; the bound turns a malicious pointer cycle into a clean error.
const MAX_POINTER_HOPS: u32 = 128;

/// A domain name: a sequence of labels (each up to 63 bytes), stored as raw bytes.
///
/// Labels are kept as raw `Vec<u8>` (not `String`) — dual-use: a non-UTF-8 label is
/// preserved rather than rejected. On decode, compression pointers (RFC 1035 §4.1.4) are
/// followed inline, so a decoded `Name` is always fully resolved. On encode, names are
/// written **uncompressed** by default ([`to_bytes`](crate::Message::to_bytes)); the
/// [`to_compressed_bytes`](crate::Message::to_compressed_bytes) path emits suffix pointers
/// via a shared [`CompressionDict`] carried in the sink's scratch.
///
/// Modeled as a `#[bin(codec = …)]` newtype: the label codec travels with the type, so a
/// `Name` is a plain field anywhere (mark it `#[brw(variable)]` in a fixed parent).
///
/// # Examples
///
/// ```
/// use dns::Name;
///
/// let n: Name = "example.com".parse().unwrap();
/// assert_eq!(n.to_string(), "example.com");
/// assert_eq!(n.labels().len(), 2);
/// assert!(Name::root().is_root());
/// ```
//~ models rfc1035#3.1 part="Name space definitions"
#[bin(codec(parse = decode_labels, write = encode_labels))]
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Name(Vec<Vec<u8>>);

impl Name {
    /// The root name (a single empty label — the DNS zero-length terminator).
    #[must_use]
    pub fn root() -> Self {
        Name(Vec::new())
    }

    /// Whether this is the root name (no labels).
    #[must_use]
    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    /// The labels, each as raw bytes.
    #[must_use]
    pub fn labels(&self) -> &[Vec<u8>] {
        &self.0
    }

    /// The name's on-wire length **uncompressed** (each label = 1 length byte + its
    /// bytes, plus the 1-byte root terminator).
    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.0.iter().map(|l| l.len() + 1).sum::<usize>() + 1
    }

    /// Render as a dotted string, lossily decoding each label as UTF-8 (for display).
    /// The root name renders as `"."`.
    #[must_use]
    pub fn to_dotted(&self) -> String {
        if self.0.is_empty() {
            return ".".to_string();
        }
        self.0
            .iter()
            .map(|l| String::from_utf8_lossy(l))
            .collect::<Vec<_>>()
            .join(".")
    }
}

impl core::fmt::Display for Name {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.to_dotted())
    }
}

impl core::str::FromStr for Name {
    type Err = crate::DnsError;

    /// Parse a dotted name (`"www.example.com"`). A trailing dot / the empty string is the
    /// root. Errors ([`NotRepresentable`](crate::DnsError::NotRepresentable)) on a label over
    /// 63 bytes, an **empty interior label** (`"a..b"` — which wouldn't round-trip, since an
    /// empty label is the on-wire root terminator), or a total name over 255 bytes.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.strip_suffix('.').unwrap_or(s);
        if s.is_empty() {
            return Ok(Name::root());
        }
        let mut labels = Vec::new();
        let mut name_len = 1usize; // the root terminator
        for part in s.split('.') {
            let bytes = part.as_bytes();
            if bytes.is_empty() {
                return Err(crate::DnsError::NotRepresentable(format!(
                    "empty label in {s:?}: a name may not contain an empty interior label"
                )));
            }
            if bytes.len() > 63 {
                return Err(crate::DnsError::NotRepresentable(format!(
                    "label {part:?} is {} bytes; the maximum is 63",
                    bytes.len()
                )));
            }
            name_len += 1 + bytes.len();
            if name_len > MAX_NAME_LEN {
                return Err(crate::DnsError::NotRepresentable(format!(
                    "name {s:?} exceeds the {MAX_NAME_LEN}-byte limit"
                )));
            }
            labels.push(bytes.to_vec());
        }
        Ok(Name(labels))
    }
}

/// The label-list decoder, following compression pointers inline (RFC 1035 §4.1.4).
///
/// On a pointer (`0b11` marker), the 14-bit offset is a byte offset **from the start of
/// the message** — so this assumes the `Source`'s bit position is message-relative, which
/// holds when decoding a whole message from a slice (`decode_exact`).
//~ implements rfc1035#4.1.4 part="Message compression — pointer following"
fn decode_labels<S: Source>(r: &mut S) -> Result<Vec<Vec<u8>>, BitError> {
    let mut labels = Vec::new();
    let mut return_pos: Option<usize> = None;
    let mut hops = 0u32;
    let mut name_len = 1usize; // the root terminator that always ends the name
    loop {
        let first = r.read::<u8>()?;
        match first >> 6 {
            0b11 => {
                // Compression pointer: this byte's low 6 bits + the next byte = 14-bit offset.
                let second = r.read::<u8>()?;
                let offset = ((usize::from(first & 0x3F)) << 8) | usize::from(second);
                // Remember where to resume once (right after the first pointer we take).
                if return_pos.is_none() {
                    return_pos = Some(r.bit_pos());
                }
                hops += 1;
                if hops > MAX_POINTER_HOPS {
                    return Err(BitError::convert(
                        "DNS name: compression pointer loop (too many hops)".to_string(),
                        r.bit_pos(),
                    ));
                }
                r.seek_to_bit(offset * 8)?;
            }
            0b00 => {
                if first == 0 {
                    break; // root terminator
                }
                // Each label costs its length octet + its bytes; cap the running total at
                // the RFC limit so compression can't amplify one name past 255 bytes.
                name_len += 1 + usize::from(first);
                if name_len > MAX_NAME_LEN {
                    return Err(BitError::convert(
                        format!("DNS name exceeds the {MAX_NAME_LEN}-byte limit (RFC 1035 §3.1)"),
                        r.bit_pos(),
                    ));
                }
                let label = r.read_bytes(usize::from(first))?;
                labels.push(label);
            }
            marker => {
                return Err(BitError::convert(
                    format!("DNS name: reserved label marker 0b{marker:02b}"),
                    r.bit_pos(),
                ));
            }
        }
    }
    if let Some(pos) = return_pos {
        r.seek_to_bit(pos)?;
    }
    Ok(labels)
}

/// A message-scoped **name-compression dictionary** (RFC 1035 §4.1.4): every label-suffix
/// written during an encode is recorded at its byte offset, so a later name ending in the
/// same suffix is emitted as a 14-bit pointer to the first occurrence instead of repeating
/// the labels.
///
/// Seed one into the sink's scratch (via `BitWriter::with_scratch`) to make
/// [`encode_labels`] compress — that's what [`Message::to_compressed_bytes`] does. Without
/// it, encoding stays uncompressed. Matching is exact (case-sensitive) for now — always
/// valid, if occasionally less compact than the RFC's case-insensitive comparison.
///
/// [`Message::to_compressed_bytes`]: crate::Message::to_compressed_bytes
#[derive(Debug, Default)]
pub struct CompressionDict {
    offsets: HashMap<Vec<Vec<u8>>, u16>,
}

impl CompressionDict {
    /// A fresh, empty dictionary.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The byte offset of a prior occurrence of exactly this label-suffix, if recorded.
    fn lookup(&self, suffix: &[Vec<u8>]) -> Option<u16> {
        self.offsets.get(suffix).copied()
    }

    /// Record this suffix at `offset` (keeping the first occurrence), but only when the
    /// offset fits the 14-bit pointer field — a name past `0x3FFF` can't be pointed to
    /// (it still encodes, just uncompressed, and earlier pointers remain valid).
    fn record(&mut self, suffix: Vec<Vec<u8>>, offset: usize) {
        if offset < 0x4000 {
            self.offsets.entry(suffix).or_insert(offset as u16);
        }
    }
}

/// The label-list encoder. **Uncompressed** by default; if the sink carries a
/// [`CompressionDict`] scratch, a label-suffix that was written earlier in the message is
/// emitted as a 14-bit compression pointer instead (RFC 1035 §4.1.4). A label over 63
/// bytes cannot be represented and is refused (its length byte would collide with the
/// pointer/marker bit space).
//~ implements rfc1035#4.1.4 part="Message compression — pointer emission"
fn encode_labels<K: Sink>(labels: &[Vec<u8>], w: &mut K) -> Result<(), BitError> {
    for (i, label) in labels.iter().enumerate() {
        if label.len() > 63 {
            return Err(BitError::convert(
                format!("DNS label is {} bytes; the maximum is 63", label.len()),
                w.bit_pos(),
            ));
        }
        let offset = w.bit_pos() / 8;
        // Consult/extend the compression dictionary if the sink carries one. The scratch
        // borrow is scoped (the pointer copied out) so it's dropped before writing to `w`.
        let pointer = {
            match w
                .scratch()
                .and_then(|s| s.downcast_mut::<CompressionDict>())
            {
                Some(dict) => match dict.lookup(&labels[i..]) {
                    Some(off) => Some(off),
                    None => {
                        dict.record(labels[i..].to_vec(), offset);
                        None
                    }
                },
                None => None,
            }
        };
        if let Some(off) = pointer {
            // A pointer both replaces this suffix and terminates the name.
            return w.write(0xC000u16 | off);
        }
        w.write(label.len() as u8)?;
        w.write_bytes(label)?;
    }
    w.write(0u8) // root terminator (no pointer taken)
}

/// Write a [`Name`] **uncompressed**, ignoring any compression dictionary in the sink.
///
/// Names *inside RDATA* (a CNAME/NS/PTR target, an MX exchange, the SOA MNAME/RNAME) are
/// written uncompressed even in [`Message::to_compressed_bytes`](crate::Message::to_compressed_bytes)
/// — RFC 3597 §4: only the **owner** name (and the question name) are compression-eligible.
/// If an RDATA name compressed, its `rdlength` (derived from the *uncompressed* size via
/// `#[bw(auto = bytes(data))]`) would no longer match the bytes written, producing a frame
/// that an RFC-compliant peer misparses. Writing them uncompressed keeps `rdlength` honest.
///
/// # Errors
/// [`BitError`] on a label over 63 bytes, or a sink write failure.
pub fn write_name_uncompressed<K: Sink>(name: &Name, w: &mut K) -> Result<(), BitError> {
    for label in name.labels() {
        if label.len() > 63 {
            return Err(BitError::convert(
                format!("DNS label is {} bytes; the maximum is 63", label.len()),
                w.bit_pos(),
            ));
        }
        w.write(label.len() as u8)?;
        w.write_bytes(label)?;
    }
    w.write(0u8) // root terminator
}

/// Pure name logic — parsing/rendering dotted strings, no wire codec.
#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn from_str_and_display() {
        let n: Name = "www.example.com".parse().unwrap();
        assert_eq!(n.labels().len(), 3);
        assert_eq!(n.to_string(), "www.example.com");
        assert_eq!(
            "example.com.".parse::<Name>().unwrap().to_dotted(),
            "example.com"
        );
        assert!("".parse::<Name>().unwrap().is_root());
    }

    #[test]
    fn root_is_a_single_dot() {
        assert!(Name::root().is_root());
        assert_eq!(Name::root().to_dotted(), ".");
    }

    #[test]
    fn byte_len_counts_labels_plus_terminator() {
        let n: Name = "a.bc".parse().unwrap();
        // (1+1) + (1+2) + 1 terminator = 6
        assert_eq!(n.byte_len(), 6);
    }

    #[test]
    fn from_str_rejects_an_oversized_label() {
        let long = "a".repeat(64);
        assert!(long.parse::<Name>().is_err());
    }

    #[test]
    fn from_str_rejects_empty_interior_labels() {
        // An empty label is the wire root terminator, so "a..b" wouldn't round-trip.
        assert!("a..b".parse::<Name>().is_err());
        assert!(".a".parse::<Name>().is_err());
        // A trailing dot is still the fully-qualified form, not an empty label.
        assert_eq!("a.b.".parse::<Name>().unwrap().to_dotted(), "a.b");
    }

    #[test]
    fn from_str_rejects_a_name_over_255_bytes() {
        // Six 63-byte labels = 6 × 64 + 1 = 385 bytes > 255.
        let big = std::iter::repeat_n("a".repeat(63), 6)
            .collect::<Vec<_>>()
            .join(".");
        assert!(big.parse::<Name>().is_err());
    }
}

/// The label codec through the bnb `Source`/`Sink` seam — including compression-pointer
/// following on decode and uncompressed encoding.
#[cfg(test)]
mod component {
    use super::*;
    use bnb::bitstream::{BitReader, BitWriter};

    #[test]
    fn simple_name_round_trips() {
        let wire = [
            0x07, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 0x03, b'c', b'o', b'm', 0x00,
        ];
        let n = Name::decode_exact(&wire).unwrap();
        assert_eq!(n.to_dotted(), "example.com");
        assert_eq!(n.to_bytes().unwrap(), wire);
    }

    #[test]
    fn root_name_round_trips() {
        let n = Name::decode_exact(&[0x00]).unwrap();
        assert!(n.is_root());
        assert_eq!(n.to_bytes().unwrap(), [0x00]);
    }

    #[test]
    fn follows_a_compression_pointer_and_resumes() {
        // offset 0: "example.com\0"; offset 13: "www" + pointer(0x0000).
        let mut buf = vec![
            0x07, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 0x03, b'c', b'o', b'm', 0x00,
        ];
        buf.extend_from_slice(&[0x03, b'w', b'w', b'w', 0xC0, 0x00]);
        let mut r = BitReader::new(&buf);
        r.seek_to_bit(13 * 8).unwrap();
        let n = Name(decode_labels(&mut r).unwrap());
        assert_eq!(n.to_dotted(), "www.example.com");
        // Resumed right after the 2 pointer bytes (offset 19).
        assert_eq!(r.bit_pos(), 19 * 8);
    }

    #[test]
    fn pointer_loop_is_bounded_not_hung() {
        let err = Name::decode_exact(&[0xC0, 0x00]).unwrap_err();
        assert!(
            matches!(&err.kind, bnb::bitstream::ErrorKind::Convert { message } if message.contains("loop")),
            "got {err:?}"
        );
    }

    #[test]
    fn oversized_label_is_refused_on_encode() {
        let n = Name(vec![vec![b'a'; 64]]);
        let mut w = BitWriter::new();
        assert!(encode_labels(n.labels(), &mut w).is_err());
    }

    #[test]
    fn a_repeated_suffix_becomes_a_pointer() {
        // Write "example.com" at offset 0, then "www.example.com": the shared
        // "example.com" suffix must compress to a pointer back to offset 0.
        let mut w = BitWriter::new().with_scratch(Box::new(CompressionDict::new()));
        encode_labels("example.com".parse::<Name>().unwrap().labels(), &mut w).unwrap();
        encode_labels("www.example.com".parse::<Name>().unwrap().labels(), &mut w).unwrap();
        let bytes = w.into_bytes();
        assert_eq!(
            &bytes[..13],
            &[
                0x07, b'e', b'x', b'a', b'm', b'p', b'l', b'e', 0x03, b'c', b'o', b'm', 0x00
            ]
        );
        // "www" then a pointer (0xC0 0x00) to example.com at offset 0.
        assert_eq!(&bytes[13..], &[0x03, b'w', b'w', b'w', 0xC0, 0x00]);
    }

    #[test]
    fn with_scratch_but_no_repeat_matches_the_uncompressed_form() {
        let name: Name = "example.com".parse().unwrap();
        let mut compressed = BitWriter::new().with_scratch(Box::new(CompressionDict::new()));
        encode_labels(name.labels(), &mut compressed).unwrap();
        // A first, unique occurrence is written in full (nothing to point at yet).
        assert_eq!(compressed.into_bytes(), name.to_bytes().unwrap());
    }

    #[test]
    fn offsets_past_the_14_bit_limit_are_not_recorded() {
        let mut dict = CompressionDict::new();
        dict.record(vec![b"com".to_vec()], 0x4000); // just past the limit
        assert_eq!(dict.lookup(&[b"com".to_vec()]), None);
        dict.record(vec![b"net".to_vec()], 0x3FFF); // the last valid offset
        assert_eq!(dict.lookup(&[b"net".to_vec()]), Some(0x3FFF));
    }
}
