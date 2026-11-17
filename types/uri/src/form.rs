//! Percent-encoding of form and query data.
//!
//! Encoders disagree about which bytes may be left as they are (`*` and `~` are the usual
//! disagreements) and about how a space is written. Where the encoded bytes are compared or
//! signed, those choices are part of the protocol, so an [`Encoding`] states both and there
//! is no default.

use alloc::vec::Vec;

/// A set of byte values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ByteSet([u64; 4]);

impl ByteSet {
    /// No bytes.
    pub const EMPTY: Self = Self([0; 4]);

    /// The unreserved characters of RFC 3986 §2.3: ASCII letters, digits, and `-`, `.`, `_`,
    /// `~`. They never need encoding anywhere in a URI.
    pub const UNRESERVED: Self = Self::EMPTY
        .with(b"ABCDEFGHIJKLMNOPQRSTUVWXYZ")
        .with(b"abcdefghijklmnopqrstuvwxyz")
        .with(b"0123456789")
        .with(b"-._~");

    /// This set with every byte of `bytes` added.
    #[must_use]
    pub const fn with(mut self, bytes: &[u8]) -> Self {
        let mut at = 0;
        while at < bytes.len() {
            let byte = bytes[at];
            self.0[(byte >> 6) as usize] |= 1 << (byte & 63);
            at += 1;
        }
        self
    }

    /// This set without any byte of `bytes`.
    #[must_use]
    pub const fn without(mut self, bytes: &[u8]) -> Self {
        let mut at = 0;
        while at < bytes.len() {
            let byte = bytes[at];
            self.0[(byte >> 6) as usize] &= !(1 << (byte & 63));
            at += 1;
        }
        self
    }

    /// Whether `byte` is in the set.
    #[must_use]
    pub const fn contains(&self, byte: u8) -> bool {
        self.0[(byte >> 6) as usize] & (1 << (byte & 63)) != 0
    }
}

/// How an encoder writes a space that is not in its `keep` set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Space {
    /// As `+`, the form-data convention.
    Plus,
    /// As `%20`, like any other encoded byte.
    Percent,
}

/// One way of percent-encoding: which bytes are kept, and how a space is written. Every
/// other byte becomes `%` and two uppercase hexadecimal digits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Encoding {
    /// The bytes written as they are.
    pub keep: ByteSet,
    /// How a space is written when `keep` does not contain it.
    pub space: Space,
}

impl Encoding {
    /// Form and query encoding as most server-side libraries write it: the RFC 3986
    /// unreserved characters are kept, a space becomes `+`, and everything else, `*`
    /// included, is percent-encoded. Python's `urllib.parse.quote_plus` and Go's
    /// `url.QueryEscape` produce exactly this.
    pub const QUERY: Self = Self {
        keep: ByteSet::UNRESERVED,
        space: Space::Plus,
    };

    /// Appends the encoding of `input` to `output`.
    pub fn component(&self, input: &[u8], output: &mut Vec<u8>) {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        for &byte in input {
            if self.keep.contains(byte) {
                output.push(byte);
            } else if byte == b' ' && self.space == Space::Plus {
                output.push(b'+');
            } else {
                output.extend([
                    b'%',
                    HEX[usize::from(byte >> 4)],
                    HEX[usize::from(byte & 15)],
                ]);
            }
        }
    }

    /// Encodes name and value pairs as `name=value` joined by `&`, in the order given.
    ///
    /// The order is the caller's: a protocol that signs or compares the result decides
    /// whether pairs are sorted, by what, and what a repeated name means.
    #[must_use]
    pub fn pairs<'a>(&self, pairs: impl IntoIterator<Item = (&'a [u8], &'a [u8])>) -> Vec<u8> {
        let mut output = Vec::new();
        for (name, value) in pairs {
            if !output.is_empty() {
                output.push(b'&');
            }
            self.component(name, &mut output);
            output.push(b'=');
            self.component(value, &mut output);
        }
        output
    }
}
