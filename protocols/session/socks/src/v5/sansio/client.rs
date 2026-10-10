// --- Standard library ---
use std::{io, mem};

// --- Workspace dependencies ---
use bnb::{BitBuf, BitDecode, BitEncode, BitError, ErrorKind};

// --- Internal modules ---
use crate::error::Error;
use crate::v5::{
    AuthMethod, Command, Endpoint, MethodRequest, MethodSelection, Reply, Request,
    UsernamePasswordRequest, UsernamePasswordResponse, VERSION, auth::ClientAuth,
};

/// The bound on retained handshake input, in bytes.
///
/// RFC 1929 §2: VER + ULEN + 255 username bytes + PLEN + 255 password bytes.
/// This also covers every RFC 1928 greeting, selection, request, and reply.
pub(crate) const MAX_FRAME_LEN: usize = 513;

/// The handshake message the client expects or sends next.
enum Phase {
    /// Sending the method offer.
    Greeting,
    /// Awaiting the server's method selection.
    Selection,
    /// Sending RFC 1929 credentials.
    Credentials,
    /// Awaiting the RFC 1929 status.
    Authentication,
    /// Sending the command request.
    Request,
    /// Awaiting the command reply.
    Reply,
    /// The reply succeeded; the bound endpoint and unread input await handoff.
    Established,
    /// A terminal error occurred or the result was taken.
    Closed,
}

/// The transport action a driver performs before calling [`Client::advance`] again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Write [`Client::transmit`] and report accepted bytes with [`Client::sent`].
    /// Flush buffering transports before waiting for the reply.
    Transmit,
    /// Read at most [`Client::receive_limit`] bytes into [`Client::receive`], or
    /// report end of stream with [`Client::receive_eof`].
    Receive,
    /// The handshake succeeded; take the result with [`Client::finish`].
    Established,
}

/// A SOCKS5 CONNECT client handshake that performs no I/O.
///
/// Exactly one method is offered. Input is bounded to one handshake message; bytes the
/// proxy sends after its reply stay buffered and are returned by [`finish`](Self::finish).
/// Every [`advance`](Self::advance) error is terminal; a [`receive`](Self::receive) capacity
/// error retains nothing and may be retried with less input. Credentials and buffers are
/// not zeroized on drop.
///
/// ```
/// use socks::v5::{Endpoint, auth::ClientAuth, sansio::{Client, Step}};
///
/// let dest = Endpoint::domain(b"example.com", 443)?;
/// let mut client = Client::new(dest, ClientAuth::NoAuthentication)?;
/// assert_eq!(client.advance()?, Step::Transmit);
/// assert_eq!(client.transmit(), [5, 1, 0]);
/// client.sent(3);
/// assert_eq!(client.advance()?, Step::Receive);
/// client.receive(&[5, 0])?;
/// assert_eq!(client.advance()?, Step::Transmit); // The CONNECT request.
/// let request = client.transmit().len();
/// client.sent(request);
///
/// // The reply arrives with the first application bytes in the same read.
/// client.receive(&[5, 0, 0, 1, 192, 0, 2, 1, 0x1f, 0x90, b'h', b'i'])?;
/// assert_eq!(client.advance()?, Step::Established);
/// let (input, bound) = client.finish()?;
/// assert_eq!(bound.port(), 8080);
/// assert_eq!(input.bit_len(), 16); // Hand both bytes to the application first.
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct Client {
    /// The message being sent or awaited.
    phase: Phase,
    /// The single authentication method offered.
    method: AuthMethod,
    /// Encoded RFC 1929 credentials, sent once the proxy selects them.
    credentials: Option<Vec<u8>>,
    /// The encoded command request, sent once authentication completes.
    request: Vec<u8>,
    /// The proxy's bound endpoint from a successful reply.
    bound: Option<Endpoint>,
    /// Encoded bytes of the message being sent.
    output: Vec<u8>,
    /// How many `output` bytes the driver has written.
    written: usize,
    /// Accepted input not yet decoded, including bytes after the reply.
    input: BitBuf,
    /// Additional bytes bnb needs before another decode attempt can succeed.
    missing: usize,
    /// Whether the proxy closed its side; the next decode treats input as finite.
    eof: bool,
}

impl Client {
    /// Validate the destination and authentication, then queue the method offer.
    /// Domains are sent to the proxy unchanged, not resolved locally.
    ///
    /// # Errors
    /// Returns invalid credential, endpoint, or codec errors.
    pub fn new(dest: Endpoint, auth: ClientAuth<'_>) -> Result<Self, Error> {
        dest.validate_destination()?;
        let credentials = auth.credentials()?;
        Self::prepared(dest, Command::Connect, auth.method(), credentials)
    }

    /// Queue the method offer for arguments the caller has already validated.
    /// BIND drivers read the second reply from the handed-off input themselves.
    pub(crate) fn prepared(
        dest: Endpoint,
        command: Command,
        method: AuthMethod,
        credentials: Option<UsernamePasswordRequest>,
    ) -> Result<Self, Error> {
        let offer = MethodRequest {
            version: VERSION,
            methods: vec![method],
        };
        let request = Request {
            version: VERSION,
            reserved: 0,
            command,
            destination: dest,
        };
        Ok(Self {
            phase: Phase::Greeting,
            method,
            credentials: credentials
                .map(|credentials| encode(&credentials))
                .transpose()?,
            request: encode(&request)?,
            bound: None,
            output: encode(&offer)?,
            written: 0,
            input: BitBuf::bounded(MAX_FRAME_LEN),
            missing: 0,
            eof: false,
        })
    }

    /// Process accepted input and report the next transport action.
    ///
    /// # Errors
    /// Returns a codec, protocol, authentication, or reply error, or `InvalidState`
    /// after a previous error or [`finish`](Self::finish). Errors are terminal.
    pub fn advance(&mut self) -> Result<Step, Error> {
        let result = self.drive();
        if result.is_err() {
            self.phase = Phase::Closed;
        }
        result
    }

    /// Unsent bytes of the current message; empty unless [`Step::Transmit`] is pending.
    #[must_use]
    pub fn transmit(&self) -> &[u8] {
        &self.output[self.written..]
    }

    /// Record that the driver wrote `count` bytes from the front of [`transmit`](Self::transmit).
    ///
    /// # Panics
    /// Panics if `count` exceeds the unsent length.
    pub fn sent(&mut self, count: usize) {
        assert!(
            count <= self.transmit().len(),
            "sent more bytes than were pending"
        );
        self.written += count;
    }

    /// The most input [`receive`](Self::receive) accepts now.
    #[must_use]
    pub fn receive_limit(&self) -> usize {
        MAX_FRAME_LEN - self.input.bit_len() / 8
    }

    /// Accept bytes read from the proxy, including any that follow its reply.
    ///
    /// # Errors
    /// Returns `InvalidState` after a terminal error or handoff, and a `BufferFull` codec
    /// error when `bytes` exceeds [`receive_limit`](Self::receive_limit); nothing is retained.
    pub fn receive(&mut self, bytes: &[u8]) -> Result<(), Error> {
        if matches!(self.phase, Phase::Closed) {
            return Err(Error::InvalidState);
        }
        self.input.push(bytes).map_err(|full| {
            Error::Codec(BitError::new(ErrorKind::BufferFull { cap: full.cap }, 0))
        })?;
        self.missing = self.missing.saturating_sub(bytes.len());
        Ok(())
    }

    /// Record that the proxy closed its side; the next message must be complete already.
    pub fn receive_eof(&mut self) {
        self.eof = true;
    }

    /// Take the unread input and the bound endpoint once the handshake is established.
    /// Hand the input to the application before reading the transport again.
    ///
    /// # Errors
    /// Returns `InvalidState` before establishment, after an error, or after a prior call.
    pub fn finish(&mut self) -> Result<(BitBuf, Endpoint), Error> {
        if !matches!(self.phase, Phase::Established) {
            return Err(Error::InvalidState);
        }
        self.phase = Phase::Closed;
        let bound = self.bound.take().ok_or(Error::InvalidState)?;
        Ok((mem::take(&mut self.input), bound))
    }

    /// Advance through every phase the buffered input and sent output allow.
    fn drive(&mut self) -> Result<Step, Error> {
        loop {
            match self.phase {
                Phase::Greeting | Phase::Credentials | Phase::Request => {
                    if !self.transmit().is_empty() {
                        return Ok(Step::Transmit);
                    }
                    self.phase = match self.phase {
                        Phase::Greeting => Phase::Selection,
                        Phase::Credentials => Phase::Authentication,
                        _ => Phase::Reply,
                    };
                }
                Phase::Selection => {
                    let Some(selection) = self.pull::<MethodSelection>()? else {
                        return Ok(Step::Receive);
                    };
                    selection.check_offered(self.method)?;
                    match self.credentials.take() {
                        Some(credentials) => self.queue(credentials, Phase::Credentials),
                        None => self.queue_request(),
                    }
                }
                Phase::Authentication => {
                    let Some(status) = self.pull::<UsernamePasswordResponse>()? else {
                        return Ok(Step::Receive);
                    };
                    status.ensure_success()?;
                    self.queue_request();
                }
                Phase::Reply => {
                    let Some(reply) = self.pull::<Reply>()? else {
                        return Ok(Step::Receive);
                    };
                    reply.ensure_success()?;
                    self.bound = Some(reply.bound);
                    self.phase = Phase::Established;
                }
                Phase::Established => return Ok(Step::Established),
                Phase::Closed => return Err(Error::InvalidState),
            }
        }
    }

    /// Queue the command request once authentication is complete.
    fn queue_request(&mut self) {
        let request = mem::take(&mut self.request);
        self.queue(request, Phase::Request);
    }

    /// Make `bytes` the pending output and enter its sending phase.
    fn queue(&mut self, bytes: Vec<u8>, phase: Phase) {
        self.output = bytes;
        self.written = 0;
        self.phase = phase;
    }

    /// Decode the next message, or `None` until enough input has arrived.
    /// Decoding is skipped while bnb's hint says the attempt cannot complete.
    ///
    /// This mirrors `bnb::net::read_message` without a transport. It omits that reader's
    /// hint reset on buffer rebase and its trailing-bit alignment: every SOCKS5 message
    /// is whole bytes and none uses absolute positions.
    fn pull<T: BitDecode + BitEncode>(&mut self) -> Result<Option<T>, Error> {
        if self.eof {
            return self.input.pull_eof()?.map(Some).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "connection closed before a message",
                )
                .into()
            });
        }
        if self.missing > 0 {
            return Ok(None);
        }
        match self.input.try_pull() {
            Ok(message) => Ok(Some(message)),
            Err(BitError {
                kind: ErrorKind::Incomplete { needed },
                ..
            }) if self.receive_limit() > 0 => {
                self.missing = needed.unwrap_or(1).max(1);
                Ok(None)
            }
            Err(error) if error.is_incomplete() => Err(Error::Codec(BitError::new(
                ErrorKind::BufferFull { cap: MAX_FRAME_LEN },
                error.at,
            ))),
            Err(error) => Err(error.into()),
        }
    }
}

/// Encode one handshake message in its own layout.
fn encode<T: BitEncode>(message: &T) -> Result<Vec<u8>, Error> {
    Ok(bnb::bitstream::encode_to_vec(message, T::LAYOUT)?)
}

#[cfg(test)]
mod unit {
    // --- Standard library ---
    use std::net::Ipv4Addr;

    // --- Internal modules ---
    use super::*;
    use crate::v5::ReplyCode;

    /// A successful IPv4 reply bound to 192.0.2.1:8080.
    const REPLY: [u8; 10] = [5, 0, 0, 1, 192, 0, 2, 1, 0x1f, 0x90];

    fn connect(auth: ClientAuth<'_>) -> Client {
        Client::new(Endpoint::addr(Ipv4Addr::LOCALHOST, 80), auth).unwrap()
    }

    /// Send everything pending and return the bytes as one message.
    fn drain(client: &mut Client) -> Vec<u8> {
        assert_eq!(client.advance().unwrap(), Step::Transmit);
        let bytes = client.transmit().to_vec();
        client.sent(bytes.len());
        bytes
    }

    /// Feed `bytes` one at a time, requiring `Receive` until the last byte.
    fn trickle(client: &mut Client, bytes: &[u8]) -> Result<Step, Error> {
        let (last, prefix) = bytes.split_last().unwrap();
        for byte in prefix {
            client.receive(&[*byte])?;
            assert_eq!(client.advance()?, Step::Receive);
        }
        client.receive(&[*last])?;
        client.advance()
    }

    #[test]
    fn byte_at_a_time_exchange_hands_off_trailing_input() {
        let mut client = connect(ClientAuth::NoAuthentication);
        assert_eq!(drain(&mut client), [5, 1, 0]);
        assert_eq!(trickle(&mut client, &[5, 0]).unwrap(), Step::Transmit);
        assert_eq!(drain(&mut client), [5, 1, 0, 1, 127, 0, 0, 1, 0, 80]);
        let mut reply = REPLY.to_vec();
        reply.extend_from_slice(b"data");
        client.receive(&reply).unwrap();
        assert_eq!(client.advance().unwrap(), Step::Established);
        let (mut input, bound) = client.finish().unwrap();
        assert_eq!(bound, Endpoint::addr(Ipv4Addr::new(192, 0, 2, 1), 8080));
        assert_eq!(input.bit_len(), 32);
        assert_eq!(
            input.pull::<u32>().unwrap(),
            Some(u32::from_be_bytes(*b"data"))
        );
        assert!(matches!(client.finish(), Err(Error::InvalidState)));
    }

    #[test]
    fn partial_writes_resume_from_the_unsent_suffix() {
        let mut client = connect(ClientAuth::NoAuthentication);
        assert_eq!(client.advance().unwrap(), Step::Transmit);
        client.sent(1);
        assert_eq!(client.advance().unwrap(), Step::Transmit);
        assert_eq!(client.transmit(), [1, 0]);
        client.sent(2);
        assert_eq!(client.advance().unwrap(), Step::Receive);
    }

    #[test]
    fn credentials_are_sent_and_their_status_is_enforced() {
        for (status, accepted) in [(0, true), (1, false)] {
            let auth = ClientAuth::UsernamePassword {
                user: b"u",
                pass: b"p",
            };
            let mut client = connect(auth);
            assert_eq!(drain(&mut client), [5, 1, 2]);
            assert_eq!(trickle(&mut client, &[5, 2]).unwrap(), Step::Transmit);
            assert_eq!(drain(&mut client), [1, 1, b'u', 1, b'p']);
            let result = trickle(&mut client, &[1, status]);
            if accepted {
                assert_eq!(result.unwrap(), Step::Transmit);
            } else {
                assert!(matches!(result, Err(Error::AuthenticationRejected)));
                assert!(matches!(client.advance(), Err(Error::InvalidState)));
            }
        }
    }

    #[test]
    fn a_failing_reply_is_terminal() {
        let mut client = connect(ClientAuth::NoAuthentication);
        drain(&mut client);
        client.receive(&[5, 0]).unwrap();
        drain(&mut client);
        let mut reply = REPLY;
        reply[1] = 5;
        client.receive(&reply).unwrap();
        assert!(matches!(
            client.advance(),
            Err(Error::Reply(ReplyCode::ConnectionRefused))
        ));
        assert!(matches!(client.receive(&[0]), Err(Error::InvalidState)));
        assert!(matches!(client.finish(), Err(Error::InvalidState)));
    }

    #[test]
    fn end_of_stream_distinguishes_silence_from_truncation() {
        let mut client = connect(ClientAuth::NoAuthentication);
        drain(&mut client);
        client.receive_eof();
        let Err(Error::Io(error)) = client.advance() else {
            panic!("an empty close is a transport error");
        };
        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);

        let mut client = connect(ClientAuth::NoAuthentication);
        drain(&mut client);
        client.receive(&[5]).unwrap();
        assert_eq!(client.advance().unwrap(), Step::Receive);
        client.receive_eof();
        assert!(matches!(client.advance(), Err(Error::Codec(_))));
    }

    #[test]
    fn oversized_input_is_rejected_without_retaining_it() {
        let mut client = connect(ClientAuth::NoAuthentication);
        drain(&mut client);
        assert_eq!(client.receive_limit(), MAX_FRAME_LEN);
        let error = client.receive(&[0; MAX_FRAME_LEN + 1]).unwrap_err();
        assert!(matches!(
            error,
            Error::Codec(BitError {
                kind: ErrorKind::BufferFull { .. },
                ..
            })
        ));
        assert_eq!(client.receive_limit(), MAX_FRAME_LEN);
        client.receive(&[5, 0]).unwrap();
        assert_eq!(client.advance().unwrap(), Step::Transmit);
    }
}
