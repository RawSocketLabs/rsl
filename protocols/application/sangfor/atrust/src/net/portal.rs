//! The control plane over real HTTPS.

// --- Standard library ---
use std::io;
use std::time::Duration;

// --- Workspace dependencies ---
use crypto::RandomSource;
use http1::blocking::{self, exchange};
use http1::{Limits, Request};
use tls::{ClientConfig, ServerVerification, blocking::Stream};

// --- Internal modules ---
use super::socket::{connect, server_name};
use crate::api::{Gateway, Portal, Reply};

/// The largest response body accepted. Most answers are small; a policy document can run to
/// megabytes.
const MAX_BODY: usize = 8 * 1024 * 1024;

/// A [`Portal`] that reaches a gateway over the network: one TCP connection and one TLS
/// handshake for each request. The request must use fixed-length framing and a path
/// beginning with `/` and carry exactly one `Host` matching the configured gateway.
///
/// The caller chooses TLS verification. Self-signed vendor gateways may require explicit
/// unverified mode (§2.1). The protocol's own checks cannot authenticate such a gateway.
///
/// The time limit applies to the connect and to each read or write. The exchange as a whole
/// is not bounded, nor is name resolution: a peer that keeps trickling bytes can hold a
/// request open.
#[derive(Debug)]
pub struct HttpsPortal<'a, R> {
    /// The TLS policy applied before every HTTP request.
    verification: ServerVerification<'a>,
    /// The gateway to which requests must be addressed.
    gateway: Gateway,
    /// The time allowed for the connect and for each read or write.
    timeout: Duration,
    /// The limits on a response.
    limits: Limits,
    /// The randomness TLS handshakes draw on.
    random: R,
}

impl<'a, R: RandomSource> HttpsPortal<'a, R> {
    /// The time allowed for the connect and for each read or write. It is the time the
    /// reference client allows a whole request (spec §2.2), applied here to each step.
    pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(20);

    /// A portal to `gateway`, drawing the randomness of its TLS handshakes from `random`,
    /// which must be a cryptographic source. A path policy retains its validation time;
    /// construct a fresh portal with the current time for each login.
    pub fn new(gateway: &Gateway, verification: ServerVerification<'a>, random: R) -> Self {
        Self {
            verification,
            gateway: gateway.clone(),
            timeout: Self::DEFAULT_TIMEOUT,
            limits: Limits::default().with_body(MAX_BODY),
            random,
        }
    }

    /// Allow `timeout` for the connect and for each read or write, in place of the default.
    #[must_use]
    pub const fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Apply `limits` to responses, in place of the default, all of it: the default here
    /// allows a larger body than [`Limits::default`] does, for the policy document.
    #[must_use]
    pub const fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }
}

impl<R: RandomSource> Portal for HttpsPortal<'_, R> {
    fn exchange(&mut self, request: &Request) -> io::Result<Reply> {
        request
            .validate_fixed_length()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        let mut hosts = request.headers.get_all("Host");
        let authority = self.gateway.base_host();
        let matches = hosts.next().is_some_and(|host| {
            let host = if self.gateway.port() == Gateway::DEFAULT_PORT {
                host.strip_suffix(b":443").unwrap_or(host)
            } else {
                host
            };
            host.eq_ignore_ascii_case(authority.as_bytes())
        });
        if !matches
            || hosts.next().is_some()
            || !request.target.starts_with(b"/")
            || request.target.contains(&b'#')
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a portal request must use origin-form and a single Host matching its gateway",
            ));
        }
        let socket = connect(self.gateway.host(), self.gateway.port(), self.timeout)?;
        let config = ClientConfig {
            server_name: server_name(self.gateway.host()),
            verification: self.verification,
        };
        let mut stream = Stream::connect(socket, config, &mut self.random)?;
        let peer_chain = stream
            .client()
            .peer_chain()
            .map(|chain| chain.der().to_vec())
            .unwrap_or_default();
        let response =
            exchange(&mut stream, request, self.limits).map_err(|error| match error {
                blocking::Error::Io(error) => error,
                blocking::Error::Http(error) => io::Error::new(io::ErrorKind::InvalidData, error),
            })?;
        // The response is whole; a peer that does not answer the goodbye loses nothing.
        stream.close().ok();
        Ok(Reply {
            response,
            peer_chain,
        })
    }
}
