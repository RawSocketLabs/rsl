//! The chunked transfer coding.

// --- Internal modules ---
use crate::Error;

/// Where the decoder is in the chunked grammar.
#[derive(Clone, Copy, Debug)]
enum State {
    /// In a chunk's hexadecimal size: its value so far, how many significant digits that
    /// took, and whether any digit has been read.
    Size { value: u64, digits: u8, any: bool },
    /// In whitespace after a size, which may only lead to an extension or the line's end.
    SizeWhitespace { value: u64 },
    /// In a chunk extension, which is skipped.
    Extension { value: u64 },
    /// After the CR that ends a size line.
    SizeLf { value: u64 },
    /// In a chunk's data.
    Data { remaining: u64 },
    /// After a chunk's data, before its CR.
    DataCr,
    /// After that CR, before its LF.
    DataLf,
    /// In a trailer line.
    Trailer { line_len: usize },
    /// After the CR that ends a trailer line.
    TrailerLf { line_len: usize },
    /// The last chunk and the trailer section have been read.
    Done,
    /// The input was not a chunked body.
    Failed,
}

/// An incremental decoder for a chunked message body (RFC 9112 §7.1).
///
/// It takes input in pieces of any size and appends the decoded data to a caller's buffer.
/// Chunk extensions and trailer fields are skipped, with their sizes bounded. Every line
/// must end in CRLF.
#[derive(Clone, Debug)]
pub struct ChunkedDecoder {
    state: State,
    /// The most decoded bytes the body may have.
    max_output: usize,
    /// Bytes of size padding, chunk extension or trailer skipped so far.
    skipped: usize,
}

impl ChunkedDecoder {
    /// The most significant hexadecimal digits in a chunk size: enough for any 64-bit length.
    const MAX_SIZE_DIGITS: u8 = 16;
    /// The most bytes of size padding, chunk extensions and trailer fields skipped in one
    /// body.
    const MAX_SKIPPED: usize = 8 * 1024;
    /// The state at the start of a chunk.
    const START: State = State::Size {
        value: 0,
        digits: 0,
        any: false,
    };

    /// A decoder at the start of a chunked body that may decode to at most `max_output`
    /// bytes.
    #[must_use]
    pub const fn new(max_output: usize) -> Self {
        Self {
            state: Self::START,
            max_output,
            skipped: 0,
        }
    }

    /// Whether the whole body, through the line that ends the trailer section, was read.
    #[must_use]
    pub const fn is_done(&self) -> bool {
        matches!(self.state, State::Done)
    }

    /// Decodes as much of `input` as belongs to this body, appending data to `output`, and
    /// returns how many input bytes were used. That is all of them until the body ends;
    /// bytes after its end are left for the caller.
    ///
    /// # Errors
    ///
    /// [`Error::Framing`] when the input is not a chunked body, and
    /// [`Error::BodyTooLarge`] as soon as a chunk's size line would take `output` past the
    /// limit. After an error every later call fails with [`Error::InvalidState`].
    pub fn decode(&mut self, input: &[u8], output: &mut Vec<u8>) -> Result<usize, Error> {
        if matches!(self.state, State::Failed) {
            return Err(Error::InvalidState);
        }
        let mut used = 0;
        while used < input.len() && !self.is_done() {
            if let State::Data { remaining } = self.state {
                let available = input.len() - used;
                let count = usize::try_from(remaining).map_or(available, |n| n.min(available));
                output.extend_from_slice(&input[used..used + count]);
                used += count;
                let remaining = remaining - count as u64;
                self.state = if remaining == 0 {
                    State::DataCr
                } else {
                    State::Data { remaining }
                };
                continue;
            }
            match self.step(input[used], output.len()) {
                Ok(state) => self.state = state,
                Err(error) => {
                    self.state = State::Failed;
                    return Err(error);
                }
            }
            used += 1;
        }
        Ok(used)
    }

    /// Advances over one byte that is not chunk data.
    fn step(&mut self, byte: u8, output_len: usize) -> Result<State, Error> {
        Ok(match (self.state, byte) {
            (State::Size { value, digits, .. }, _) if byte.is_ascii_hexdigit() => {
                let digit = (byte as char).to_digit(16).map_or(0, u64::from);
                // Leading zeros carry no value; they are bounded as padding instead.
                let significant = value != 0 || digit != 0;
                if !significant {
                    self.skip()?;
                } else if digits == Self::MAX_SIZE_DIGITS {
                    return Err(Error::Framing("a chunk size is too large"));
                }
                State::Size {
                    value: value << 4 | digit,
                    digits: digits + u8::from(significant),
                    any: true,
                }
            }
            (State::Size { any: false, .. }, _) => {
                return Err(Error::Framing("a chunk size is missing"));
            }
            (
                State::Size { value, .. }
                | State::SizeWhitespace { value }
                | State::Extension { value },
                b'\r',
            ) => State::SizeLf { value },
            (State::Size { value, .. } | State::SizeWhitespace { value }, b' ' | b'\t') => {
                self.skip()?;
                State::SizeWhitespace { value }
            }
            (State::Size { value, .. } | State::SizeWhitespace { value }, b';') => {
                self.skip()?;
                State::Extension { value }
            }
            (State::Extension { value }, byte) if byte != b'\n' => {
                self.skip()?;
                State::Extension { value }
            }
            (State::SizeLf { value: 0 }, b'\n') => State::Trailer { line_len: 0 },
            (State::SizeLf { value }, b'\n') => {
                let room = self.max_output.saturating_sub(output_len) as u64;
                if value > room {
                    return Err(Error::BodyTooLarge);
                }
                State::Data { remaining: value }
            }
            (State::DataCr, b'\r') => State::DataLf,
            (State::DataLf, b'\n') => Self::START,
            (State::Trailer { line_len }, b'\r') => State::TrailerLf { line_len },
            (State::Trailer { line_len }, byte) if byte != b'\n' => {
                self.skip()?;
                State::Trailer {
                    line_len: line_len + 1,
                }
            }
            (State::TrailerLf { line_len: 0 }, b'\n') => State::Done,
            (State::TrailerLf { .. }, b'\n') => State::Trailer { line_len: 0 },
            _ => return Err(Error::Framing("the body is not validly chunked")),
        })
    }

    /// Counts one skipped byte against the bound on extensions and trailers.
    fn skip(&mut self) -> Result<(), Error> {
        self.skipped += 1;
        if self.skipped > Self::MAX_SKIPPED {
            return Err(Error::Framing("chunk extensions or trailers are too long"));
        }
        Ok(())
    }
}
