//! Decoding a response from the bytes a server sends.

// --- Internal modules ---
use super::{BodyFraming, ChunkedDecoder, Response, ResponseHead};
use crate::{Error, Limits};

/// What the decoder is reading.
#[derive(Debug)]
enum State {
    /// The status line and header fields.
    Head,
    /// A body of known length, with this many bytes still to come.
    Length(u64),
    /// A chunked body.
    Chunked(ChunkedDecoder),
    /// A body that ends when the connection does.
    UntilClose,
    /// The whole response has been read.
    Done,
    /// Decoding failed.
    Failed,
}

/// A sans-I/O decoder for one response: feed it the bytes a server sends, in pieces of any
/// size, and take the [`Response`] out when it is complete.
///
/// It owns every framing decision, so a caller's read loop needs none:
///
/// - Interim (1xx) responses are skipped, up to a bound.
/// - Whether there is a body, and how long, follows RFC 9112 §6.3 from the request's method
///   and the response's head ([`BodyFraming`]).
/// - Bytes after the end of the response are not taken: [`receive`](Self::receive) returns
///   how many it used.
/// - A body that runs to the end of the connection completes only when
///   [`receive_eof`](Self::receive_eof) is told the close was clean.
#[derive(Debug)]
pub struct ResponseDecoder {
    method: Vec<u8>,
    limits: Limits,
    state: State,
    /// The head's bytes so far.
    buffer: Vec<u8>,
    /// How much of `buffer` has been searched for the end of the head.
    scanned: usize,
    interim: u8,
    head: Option<ResponseHead>,
    framing: BodyFraming,
    body: Vec<u8>,
}

impl ResponseDecoder {
    /// Interim responses skipped before the server is given up on.
    const MAX_INTERIM: u8 = 8;

    /// A decoder for the response to a request with this method. The method matters: the
    /// response to a `HEAD` has no body whatever its header fields say.
    #[must_use]
    pub fn new(request_method: &[u8], limits: Limits) -> Self {
        Self {
            method: request_method.to_vec(),
            limits,
            state: State::Head,
            buffer: Vec::new(),
            scanned: 0,
            interim: 0,
            head: None,
            framing: BodyFraming::None,
            body: Vec::new(),
        }
    }

    /// Takes bytes read from the connection and returns how many belong to this response.
    /// Fewer than offered means the response is complete and the rest belongs to whatever
    /// the server sends next.
    ///
    /// # Errors
    ///
    /// Any [`Error`] the bytes cause, after which the decoder accepts nothing more, and
    /// [`Error::InvalidState`] once decoding has failed.
    pub fn receive(&mut self, bytes: &[u8]) -> Result<usize, Error> {
        if matches!(self.state, State::Failed) {
            return Err(Error::InvalidState);
        }
        let mut taken = 0;
        while taken < bytes.len() && !self.is_complete() {
            match self.step(&bytes[taken..]) {
                Ok(count) => taken += count,
                Err(error) => return Err(self.fail(error)),
            }
        }
        Ok(taken)
    }

    /// Tells the decoder the connection has ended. `clean` says whether the transport
    /// confirmed that the server ended it deliberately: a TCP end-of-stream on a plain
    /// connection, a `close_notify` on TLS.
    ///
    /// # Errors
    ///
    /// [`Error::ClosedBeforeResponse`] when nothing of a response had arrived,
    /// [`Error::Truncated`] when the response was cut short, and [`Error::UncleanClose`]
    /// when a body that runs to the end of the connection ended without a clean close and
    /// [`Limits::accept_unclean_close`] is off.
    pub fn receive_eof(&mut self, clean: bool) -> Result<(), Error> {
        let error = match self.state {
            State::Done => return Ok(()),
            State::Failed => return Err(Error::InvalidState),
            State::UntilClose if clean || self.limits.accept_unclean_close => {
                self.state = State::Done;
                return Ok(());
            }
            State::UntilClose => Error::UncleanClose,
            State::Head if self.buffer.is_empty() && self.interim == 0 => {
                Error::ClosedBeforeResponse
            }
            State::Head | State::Length(_) | State::Chunked(_) => Error::Truncated,
        };
        Err(self.fail(error))
    }

    /// Whether the whole response has been read.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        matches!(self.state, State::Done)
    }

    /// The complete response.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidState`] when the response is not complete.
    pub fn finish(self) -> Result<Response, Error> {
        let (State::Done, Some(head)) = (self.state, self.head) else {
            return Err(Error::InvalidState);
        };
        // HTTP/1.1 or a later 1.x: persistence is not this protocol's to promise for others.
        let keep_alive = head.version.0 == 1
            && head.version.1 >= 1
            && !head.headers.has_token("Connection", "close")
            && self.framing != BodyFraming::UntilClose;
        Ok(Response {
            head,
            body: self.body,
            keep_alive,
        })
    }

    /// Whatever was decoded before the decoder stopped: the final (not interim) head, if it
    /// had arrived, and the body bytes so far. For diagnosing a response that failed or was
    /// cut short; these bytes are not a response.
    #[must_use]
    pub fn into_partial(self) -> (Option<ResponseHead>, Vec<u8>) {
        (self.head, self.body)
    }

    /// Marks the decoder failed.
    fn fail(&mut self, error: Error) -> Error {
        self.state = State::Failed;
        error
    }

    /// Consumes some of `bytes` in the current state and returns how many.
    fn step(&mut self, bytes: &[u8]) -> Result<usize, Error> {
        match &mut self.state {
            State::Head => self.receive_head(bytes),
            State::Length(remaining) => {
                let count = usize::try_from(*remaining).map_or(bytes.len(), |n| n.min(bytes.len()));
                self.body.extend_from_slice(&bytes[..count]);
                *remaining -= count as u64;
                if *remaining == 0 {
                    self.state = State::Done;
                }
                Ok(count)
            }
            State::Chunked(decoder) => {
                let count = decoder.decode(bytes, &mut self.body)?;
                if decoder.is_done() {
                    self.state = State::Done;
                }
                Ok(count)
            }
            State::UntilClose => {
                if self.body.len() + bytes.len() > self.limits.body {
                    return Err(Error::BodyTooLarge);
                }
                self.body.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            State::Done | State::Failed => Ok(0),
        }
    }

    /// Buffers head bytes up to the blank line that ends the head, then acts on the head.
    fn receive_head(&mut self, bytes: &[u8]) -> Result<usize, Error> {
        let room = self.limits.head.saturating_sub(self.buffer.len());
        if room == 0 {
            return Err(Error::HeadTooLarge);
        }
        let before = self.buffer.len();
        let offered = bytes.len().min(room);
        self.buffer.extend_from_slice(&bytes[..offered]);

        // A blank line is an LF followed by an LF or by CRLF. Stopping at either lets the
        // parser, not a full buffer, be what rejects a head whose lines end in a lone LF.
        let start = self.scanned.saturating_sub(2);
        let end = (start..self.buffer.len()).find_map(|at| match &self.buffer[at..] {
            [b'\n', b'\n', ..] => Some(at + 2),
            [b'\n', b'\r', b'\n', ..] => Some(at + 3),
            _ => None,
        });
        let Some(end) = end else {
            self.scanned = self.buffer.len();
            return Ok(offered);
        };
        self.buffer.truncate(end);
        let head = ResponseHead::parse(&self.buffer, self.limits.headers)?;
        self.buffer.clear();
        self.scanned = 0;
        self.accept_head(head)?;
        Ok(end - before)
    }

    /// Decides what follows a head: another head, a body, or nothing.
    fn accept_head(&mut self, head: ResponseHead) -> Result<(), Error> {
        if matches!(head.status, 100 | 102..=199) {
            self.interim += 1;
            if self.interim > Self::MAX_INTERIM {
                return Err(Error::TooManyInterimResponses);
            }
            return Ok(());
        }
        // Kept from here on, so a head that is refused below can still be inspected.
        let head = self.head.insert(head);
        if head.status == 101 {
            return Err(Error::Unsupported("a protocol switch"));
        }
        self.framing = BodyFraming::of_response(&self.method, head)?;
        self.state = match self.framing {
            BodyFraming::None | BodyFraming::Length(0) => State::Done,
            BodyFraming::Length(length) => {
                if length > self.limits.body as u64 {
                    return Err(Error::BodyTooLarge);
                }
                State::Length(length)
            }
            BodyFraming::Chunked => State::Chunked(ChunkedDecoder::new(self.limits.body)),
            BodyFraming::UntilClose => State::UntilClose,
        };
        Ok(())
    }
}
