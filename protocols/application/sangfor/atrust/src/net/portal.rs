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
/// handshake for each request.
///
/// **The gateway's certificate is not verified** (spec §2.1): gateways present self-signed
/// ones. Its chain is returned with each response for the protocol's own anti-MITM checks,
/// which a login reports and, unless told to be strict, does not act on.
///
/// The time limit applies to the connect and to each read or write. The exchange as a whole
/// is not bounded, nor is name resolution: a peer that keeps trickling bytes can hold a
/// request open.
#[derive(Debug)]
pub struct HttpsPortal<R> {
    /// The gateway's host.
    host: String,
    /// Its HTTPS port.
    port: u16,
    /// The time allowed for the connect and for each read or write.
    timeout: Duration,
    /// The limits on a response.
    limits: Limits,
    /// The randomness TLS handshakes draw on.
    random: R,
}

impl<R: RandomSource> HttpsPortal<R> {
    /// The time allowed for the connect and for each read or write. It is the time the
    /// reference client allows a whole request (spec §2.2), applied here to each step.
    pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(20);

    /// A portal to `gateway`, drawing the randomness of its TLS handshakes from `random`,
    /// which must be a cryptographic source.
    pub fn new(gateway: &Gateway, random: R) -> Self {
        Self {
            host: gateway.host().to_owned(),
            port: gateway.port(),
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

impl<R: RandomSource> Portal for HttpsPortal<R> {
    fn exchange(&mut self, request: &Request) -> io::Result<Reply> {
        let socket = connect(&self.host, self.port, self.timeout)?;
        let config = ClientConfig {
            server_name: server_name(&self.host),
            verification: ServerVerification::Unverified,
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
