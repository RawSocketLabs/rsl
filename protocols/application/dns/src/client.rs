//! A synchronous UDP DNS resolver client (the `client` feature).
//!
//! [`Resolver`] sends a query to a configured server over UDP and returns the decoded
//! response [`Message`], with a timeout, a few retries, and the basic anti-spoofing checks a
//! stub resolver needs (the response must come from the server, echo the query id, and have
//! the QR bit set). It's built on bnb's [`MessageDatagram`] socket helper, so it needs no
//! `rawsock` — a *dual-use* (spoofing) client is a separate, later concern.
//!
//! A truncated (TC-bit) response returns [`ResolveError::Truncated`]: DNS-over-TCP fallback
//! lands once the TCP transport exists. EDNS(0), multiple servers, and caching are follow-ups.
//!
//! ```no_run
//! use dns::{QType, Resolver};
//!
//! let resolver = Resolver::new("1.1.1.1:53".parse().unwrap());
//! for ip in resolver.resolve_ipv4("example.com").unwrap() {
//!     println!("{ip}");
//! }
//! ```

use crate::{Message, QClass, QType, Question, RData};
use bnb::bitstream::{BitError, ErrorKind};
use bnb::{DatagramSocket, MessageDatagram};
use rsl_deps::rand::Rng;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream, UdpSocket};
use std::time::Duration;

/// An error from a [`Resolver`] query.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ResolveError {
    /// The socket failed (bind, send, or receive).
    #[error("resolver I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The response could not be decoded, or another codec error occurred.
    #[error("resolver codec error: {0}")]
    Codec(#[from] BitError),
    /// The query name is not a valid domain name.
    #[error("invalid query name: {0}")]
    Name(#[from] crate::DnsError),
    /// No valid response arrived within the timeout, across all attempts.
    #[error("no response after {0} attempt(s) (timed out)")]
    Timeout(u32),
    /// The response set the TC (truncated) bit — the answer didn't fit a UDP datagram.
    /// [`query`](Resolver::query) retries such a response over TCP automatically; this
    /// surfaces only from [`query_udp`](Resolver::query_udp).
    #[error("response truncated (TC bit); retry over TCP")]
    Truncated,
    /// A TCP response didn't echo the query id, or wasn't a response (QR unset).
    #[error("TCP response did not match the query (id or QR mismatch)")]
    Mismatch,
}

/// A synchronous UDP DNS resolver client. Configure a server (and optionally the timeout and
/// retry count), then [`query`](Self::query) or use the [`resolve_ipv4`](Self::resolve_ipv4)
/// / [`resolve_ipv6`](Self::resolve_ipv6) convenience helpers.
#[derive(Debug, Clone)]
pub struct Resolver {
    server: SocketAddr,
    timeout: Duration,
    retries: u8,
}

impl Resolver {
    /// A resolver targeting `server` (e.g. `"1.1.1.1:53"`), with a 5-second per-attempt
    /// timeout and 2 retries.
    #[must_use]
    pub fn new(server: SocketAddr) -> Self {
        Self {
            server,
            timeout: Duration::from_secs(5),
            retries: 2,
        }
    }

    /// Set the per-attempt receive timeout.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set how many times to re-send after a timeout (total attempts = `retries + 1`).
    #[must_use]
    pub fn with_retries(mut self, retries: u8) -> Self {
        self.retries = retries;
        self
    }

    /// Query the server for `name`/`qtype`, over UDP, **falling back to TCP** if the UDP
    /// response is truncated (the TC bit) — a stub resolver's standard behavior (RFC 1035
    /// §4.2). For a single transport use [`query_udp`](Self::query_udp) / [`query_tcp`](Self::query_tcp).
    ///
    /// # Errors
    /// [`ResolveError`] on an invalid name, socket failure, timeout, or (from the TCP leg) a
    /// mismatched response.
    pub fn query(&self, name: &str, qtype: QType) -> Result<Message, ResolveError> {
        match self.query_udp(name, qtype) {
            Err(ResolveError::Truncated) => self.query_tcp(name, qtype),
            other => other,
        }
    }

    /// Query over **UDP only**. A truncated response is [`ResolveError::Truncated`] (it does
    /// not fall back to TCP — use [`query`](Self::query) for that).
    ///
    /// # Errors
    /// [`ResolveError`] on an invalid name, socket failure, timeout across all attempts, or a
    /// truncated response.
    pub fn query_udp(&self, name: &str, qtype: QType) -> Result<Message, ResolveError> {
        let query = self.build_query(name, qtype)?;

        // Bind an ephemeral local port in the server's address family.
        let bind: SocketAddr = if self.server.is_ipv6() {
            (Ipv6Addr::UNSPECIFIED, 0).into()
        } else {
            (Ipv4Addr::UNSPECIFIED, 0).into()
        };
        let sock = UdpSocket::bind(bind)?;
        sock.set_read_timeout(Some(self.timeout))?;

        let mut datagram = MessageDatagram::new(sock);
        self.exchange(&mut datagram, &query)
    }

    /// Query over **TCP** (RFC 1035 §4.2.2: the message is framed by a 2-byte big-endian
    /// length prefix). Used automatically by [`query`](Self::query) as the truncation
    /// fallback, and directly when TCP is required (a large query/response, a zone transfer).
    ///
    /// # Errors
    /// [`ResolveError`] on an invalid name, connection/timeout failure, a decode error, an
    /// oversized query (over 65535 bytes), or a response that doesn't match the query.
    pub fn query_tcp(&self, name: &str, qtype: QType) -> Result<Message, ResolveError> {
        let query = self.build_query(name, qtype)?;
        let query_bytes = query.to_bytes()?;
        let len = u16::try_from(query_bytes.len()).map_err(|_| {
            ResolveError::Codec(BitError::convert(
                "DNS query exceeds the 65535-byte TCP frame limit".into(),
                0,
            ))
        })?;

        let mut stream = TcpStream::connect(self.server)?;
        stream.set_read_timeout(Some(self.timeout))?;
        stream.set_write_timeout(Some(self.timeout))?;
        // Send: 2-byte length prefix, then the message.
        stream.write_all(&len.to_be_bytes())?;
        stream.write_all(&query_bytes)?;
        stream.flush()?;

        // Receive: 2-byte length prefix, then exactly that many message bytes.
        let mut len_buf = [0u8; 2];
        stream.read_exact(&mut len_buf)?;
        let resp_len = usize::from(u16::from_be_bytes(len_buf));
        let mut resp_buf = vec![0u8; resp_len];
        stream.read_exact(&mut resp_buf)?;

        let resp = Message::decode_exact(&resp_buf)?;
        if resp.header.id != query.header.id || !resp.header.is_response() {
            return Err(ResolveError::Mismatch);
        }
        Ok(resp)
    }

    /// Build a recursive query message (random id, RD set) for `name`/`qtype`.
    fn build_query(&self, name: &str, qtype: QType) -> Result<Message, ResolveError> {
        let question = Question {
            name: name.parse()?,
            qtype,
            qclass: QClass::Internet,
        };
        Ok(Message::query(
            rsl_deps::rand::rng().random::<u16>(),
            question,
        ))
    }

    /// The transport-agnostic query exchange: send, then read one response, validating it
    /// against the query; retry on timeout. Generic over the datagram socket so tests can
    /// drive it with a `MockDatagramSocket`.
    fn exchange<D: DatagramSocket<Addr = SocketAddr>>(
        &self,
        datagram: &mut MessageDatagram<D>,
        query: &Message,
    ) -> Result<Message, ResolveError> {
        let attempts = u32::from(self.retries) + 1;
        for _ in 0..attempts {
            datagram.send_message(query, &self.server)?;
            match datagram.recv_message::<Message>() {
                Ok((resp, from)) => {
                    // Reject an off-path source, a mismatched id, or a query (QR unset) —
                    // basic anti-spoofing — and re-send on the next attempt.
                    let valid = from == self.server
                        && resp.header.id == query.header.id
                        && resp.header.is_response();
                    if valid {
                        if resp.header.state.truncated() {
                            return Err(ResolveError::Truncated);
                        }
                        return Ok(resp);
                    }
                }
                // A receive timeout — try again.
                Err(e) if is_timeout(&e) => {}
                Err(e) => return Err(ResolveError::Codec(e)),
            }
        }
        Err(ResolveError::Timeout(attempts))
    }

    /// Resolve `name` to its IPv4 (A-record) addresses.
    ///
    /// # Errors
    /// As [`query`](Self::query).
    pub fn resolve_ipv4(&self, name: &str) -> Result<Vec<Ipv4Addr>, ResolveError> {
        let resp = self.query(name, QType::A)?;
        Ok(resp
            .answers
            .iter()
            .filter_map(|r| match &r.data {
                RData::A(ip) => Some(*ip),
                _ => None,
            })
            .collect())
    }

    /// Resolve `name` to its IPv6 (AAAA-record) addresses.
    ///
    /// # Errors
    /// As [`query`](Self::query).
    pub fn resolve_ipv6(&self, name: &str) -> Result<Vec<Ipv6Addr>, ResolveError> {
        let resp = self.query(name, QType::AAAA)?;
        Ok(resp
            .answers
            .iter()
            .filter_map(|r| match &r.data {
                RData::Aaaa(ip) => Some(*ip),
                _ => None,
            })
            .collect())
    }
}

/// Whether a receive error is a timeout (a UDP read timeout surfaces as `WouldBlock` on Unix,
/// `TimedOut` on Windows).
fn is_timeout(e: &BitError) -> bool {
    matches!(
        e.kind,
        ErrorKind::Io(k)
            if matches!(k, std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut)
    )
}

/// The query-validation / retry logic driven by a scripted `MockDatagramSocket` — no real
/// server, no network.
#[cfg(test)]
mod component {
    use super::*;
    use crate::{Header, RClass, RType, Record, State, WireLen};
    use bnb::MockDatagramSocket;

    fn server() -> SocketAddr {
        "127.0.0.1:53".parse().unwrap()
    }

    fn query(id: u16) -> Message {
        Message::query(
            id,
            Question {
                name: "example.com".parse().unwrap(),
                qtype: QType::A,
                qclass: QClass::Internet,
            },
        )
    }

    /// A response `Message` echoing `id`, with one A answer, optionally truncated.
    fn response(id: u16, ip: Ipv4Addr, truncated: bool) -> Message {
        let answer = Record {
            name: "example.com".parse().unwrap(),
            rtype: RType::A,
            class: RClass::Internet,
            ttl: 60,
            rdlength: WireLen::auto(),
            data: RData::A(ip),
        };
        let header = Header {
            id,
            state: State::new().with_response(true).with_truncated(truncated),
            qdcount: WireLen::auto(),
            ancount: WireLen::auto(),
            nscount: WireLen::auto(),
            arcount: WireLen::auto(),
        };
        Message::assemble(
            header,
            vec![Question {
                name: "example.com".parse().unwrap(),
                qtype: QType::A,
                qclass: QClass::Internet,
            }],
            vec![answer],
            vec![],
            vec![],
        )
    }

    fn drive(
        resolver: &Resolver,
        query: &Message,
        sock: MockDatagramSocket,
    ) -> Result<Message, ResolveError> {
        let mut md = MessageDatagram::new(sock);
        resolver.exchange(&mut md, query)
    }

    #[test]
    fn a_matching_response_is_returned() {
        let sock = MockDatagramSocket::new();
        sock.push_inbound(
            &response(0x1234, Ipv4Addr::new(1, 2, 3, 4), false)
                .to_bytes()
                .unwrap(),
            server(),
        );
        let got = drive(&Resolver::new(server()), &query(0x1234), sock).unwrap();
        assert_eq!(got.answers[0].data, RData::A(Ipv4Addr::new(1, 2, 3, 4)));
    }

    #[test]
    fn a_truncated_response_errors() {
        let sock = MockDatagramSocket::new();
        sock.push_inbound(
            &response(0x1234, Ipv4Addr::LOCALHOST, true)
                .to_bytes()
                .unwrap(),
            server(),
        );
        assert!(matches!(
            drive(&Resolver::new(server()), &query(0x1234), sock),
            Err(ResolveError::Truncated)
        ));
    }

    #[test]
    fn a_wrong_id_is_rejected_and_times_out() {
        // Response id 0x9999 doesn't echo the query's 0x1234 → ignored; no more inbound →
        // the retries exhaust and it times out (rather than accepting the spoofed answer).
        let sock = MockDatagramSocket::new();
        sock.push_inbound(
            &response(0x9999, Ipv4Addr::LOCALHOST, false)
                .to_bytes()
                .unwrap(),
            server(),
        );
        assert!(matches!(
            drive(
                &Resolver::new(server()).with_retries(0),
                &query(0x1234),
                sock
            ),
            Err(ResolveError::Timeout(1))
        ));
    }

    #[test]
    fn an_off_path_source_is_rejected() {
        let sock = MockDatagramSocket::new();
        let attacker: SocketAddr = "10.0.0.1:53".parse().unwrap();
        sock.push_inbound(
            &response(0x1234, Ipv4Addr::LOCALHOST, false)
                .to_bytes()
                .unwrap(),
            attacker,
        );
        assert!(matches!(
            drive(
                &Resolver::new(server()).with_retries(0),
                &query(0x1234),
                sock
            ),
            Err(ResolveError::Timeout(1))
        ));
    }

    #[test]
    fn no_response_times_out_after_all_attempts() {
        let sock = MockDatagramSocket::new(); // nothing queued → every recv is WouldBlock
        let err = drive(
            &Resolver::new(server()).with_retries(2),
            &query(0x1234),
            sock,
        )
        .unwrap_err();
        assert!(matches!(err, ResolveError::Timeout(3)));
    }
}
