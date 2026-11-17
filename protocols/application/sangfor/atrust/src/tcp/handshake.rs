//! Opening a tunnel: what the client sends, and how the node's answer is read.

// --- Workspace dependencies ---
use bnb::BitError;
use sangfor_atrust_wire::tcp::{ConnectReply, ReplyCode, ReplyHead};
use sangfor_atrust_wire::{AuthReply, MethodReply, Negotiation, TunnelRequest};
use serde::{Deserialize, Deserializer};
use serde_json::Value;

// --- Internal modules ---
use super::{Destination, Error, Identity, request};

/// The address type of a name, which a bound address must not be.
const NAME_TYPE: u8 = 0x03;
/// The codes with which a node says the session is gone (spec §4.4).
const SESSION_INVALID: [i64; 2] = [10_000_004, 75_500_002];

/// How the data phase of an open tunnel works (spec §9.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Mode {
    /// Whether stream data travels in frames. Otherwise the connection's bytes are the
    /// stream's.
    pub framed: bool,
    /// Whether the node agreed that the stream ends with a close frame, which leaves the
    /// connection itself open. Only ever true in framed mode.
    pub reusable: bool,
}

/// Where a handshake stands after some of the node's answer has been read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Progress {
    /// The answer is not complete. Read more from the node and pass it on.
    NeedMore,
    /// The tunnel is open.
    Open {
        /// How its data phase works.
        mode: Mode,
        /// Bytes that arrived after the answer. They are the start of the data phase and
        /// must be read before anything further from the node.
        early: Vec<u8>,
    },
}

/// The part of the node's answer being waited for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    /// The method the node accepts.
    Method,
    /// The node's verdict on the signed request.
    Auth,
    /// Whether the node connected to the destination.
    Result,
    /// The address the node bound, which ends the answer.
    Bound,
    /// Nothing: the handshake is over, one way or the other.
    Over,
}

/// The opening of one tunnel (spec §9.1), without I/O: [`new`](Self::new) gives the bytes to
/// send, and [`receive`](Self::receive) takes the node's answer as it arrives.
///
/// Nothing but the first bytes is sent until the whole answer is in, in either mode.
#[derive(Debug)]
pub struct Handshake {
    /// Whether the framed data phase was asked for.
    framed: bool,
    /// What is being waited for.
    step: Step,
    /// Bytes received and not yet consumed by a step.
    buffer: Vec<u8>,
}

impl Handshake {
    /// Start the handshake for a stream to `destination` on behalf of `identity`. `zero_rtt`
    /// is the gateway's capability from its manifest: with it the framed data phase is asked
    /// for.
    ///
    /// Returns the handshake and the bytes to send to the node, in one write.
    ///
    /// # Errors
    ///
    /// [`Error::TooLong`] when the request does not fit its length fields, and
    /// [`Error::Sig`] when signing fails.
    pub fn new(
        identity: &Identity,
        destination: &Destination,
        zero_rtt: bool,
    ) -> Result<(Self, Vec<u8>), Error> {
        let document = request::signed(identity, destination)?;
        let too_long = |_: BitError| Error::TooLong;
        let mut first = Negotiation::tcp(document).to_bytes().map_err(too_long)?;
        let connect = TunnelRequest::connect(destination.wire.clone(), zero_rtt);
        first.extend(connect.to_bytes().map_err(too_long)?);
        let handshake = Self {
            framed: zero_rtt,
            step: Step::Method,
            buffer: Vec::new(),
        };
        Ok((handshake, first))
    }

    /// Take the next bytes of the node's answer.
    ///
    /// # Errors
    ///
    /// [`Error::Protocol`] when the answer is not what the protocol allows,
    /// [`Error::SessionInvalid`] or [`Error::Refused`] when the node does not authorise the
    /// stream, and [`Error::Connect`] when it cannot reach the destination. After an error
    /// the connection is of no further use.
    pub fn receive(&mut self, bytes: &[u8]) -> Result<Progress, Error> {
        self.buffer.extend_from_slice(bytes);
        let result = self.advance();
        if result.is_err() {
            // Over, and nothing fed from here on is kept.
            self.step = Step::Over;
            self.buffer = Vec::new();
        }
        result
    }

    /// Consume as many parts of the answer as the buffer holds.
    fn advance(&mut self) -> Result<Progress, Error> {
        loop {
            let consumed = match self.step {
                Step::Method => {
                    let Some((reply, consumed)) = part(MethodReply::decode_prefix(&self.buffer))?
                    else {
                        return Ok(Progress::NeedMore);
                    };
                    let accepted = (Negotiation::VERSION, Negotiation::METHOD_TCP);
                    if (reply.version, reply.method) != accepted {
                        return Err(Error::Protocol("the node did not accept the tunnel method"));
                    }
                    self.step = Step::Auth;
                    consumed
                }
                Step::Auth => {
                    // On a refusal the node sends these two bytes and nothing more, so they
                    // are judged before the rest is waited for.
                    let Some(start) = self.buffer.get(..2) else {
                        return Ok(Progress::NeedMore);
                    };
                    if start != [Negotiation::MARKER, 0] {
                        return Err(Error::Protocol("the node refused the request outright"));
                    }
                    let Some((reply, consumed)) = part(AuthReply::decode_prefix(&self.buffer))?
                    else {
                        return Ok(Progress::NeedMore);
                    };
                    authorised(&reply.json)?;
                    self.step = Step::Result;
                    consumed
                }
                Step::Result => {
                    let Some((head, consumed)) = part(ReplyHead::decode_prefix(&self.buffer))?
                    else {
                        return Ok(Progress::NeedMore);
                    };
                    if head.code != ReplyCode::Succeeded {
                        return Err(Error::Connect(head.code));
                    }
                    self.step = Step::Bound;
                    consumed
                }
                Step::Bound => {
                    // The type byte is enough to refuse a name, without waiting for it.
                    if self.buffer.get(1) == Some(&NAME_TYPE) {
                        return Err(Error::Protocol("the node bound a name, not an address"));
                    }
                    let Some((reply, consumed)) = part(ConnectReply::decode_prefix(&self.buffer))?
                    else {
                        return Ok(Progress::NeedMore);
                    };
                    self.step = Step::Over;
                    let reusable = self.framed && reply.flag == ConnectReply::FLAG_REUSABLE;
                    let early = self.buffer.split_off(consumed);
                    self.buffer = Vec::new();
                    return Ok(Progress::Open {
                        mode: Mode {
                            framed: self.framed,
                            reusable,
                        },
                        early,
                    });
                }
                Step::Over => return Err(Error::Protocol("the handshake is over")),
            };
            self.buffer.drain(..consumed);
        }
    }
}

/// One part of the answer from the front of the buffer: the part and its length, or `None`
/// when more bytes are needed.
fn part<T>(decoded: Result<(T, usize), BitError>) -> Result<Option<(T, usize)>, Error> {
    match decoded {
        Ok(part) => Ok(Some(part)),
        Err(error) if error.is_incomplete() => Ok(None),
        Err(_) => Err(Error::Protocol("a part of the node's answer is malformed")),
    }
}

/// A member that is there, whatever it holds. Without this a null would read as absent.
fn present<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}

/// Judge the JSON of the node's verdict (spec §9.1): it must be an object, and its `code`,
/// when it has one, must be the integer zero.
fn authorised(json: &[u8]) -> Result<(), Error> {
    /// The members of the verdict that are read.
    #[derive(Deserialize)]
    struct Verdict {
        /// The result code. Absent counts as zero; present, it must be an integer, and a
        /// null is present.
        #[serde(default, deserialize_with = "present")]
        code: Option<Value>,
        /// The node's message, used only to say why it refused.
        #[serde(default)]
        message: Option<Value>,
    }
    let not_a_verdict = Error::Protocol("the node's verdict is not a JSON object with a code");
    let is_object = json.iter().find(|byte| !byte.is_ascii_whitespace()) == Some(&b'{');
    let Some(verdict) = is_object
        .then(|| serde_json::from_slice::<Verdict>(json).ok())
        .flatten()
    else {
        return Err(not_a_verdict);
    };
    let code = match verdict.code {
        None => 0,
        Some(code) => code.as_i64().ok_or(not_a_verdict)?,
    };
    match code {
        0 => Ok(()),
        code if SESSION_INVALID.contains(&code) => Err(Error::SessionInvalid { code }),
        code => Err(Error::Refused {
            code,
            message: verdict
                .message
                .as_ref()
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        }),
    }
}
