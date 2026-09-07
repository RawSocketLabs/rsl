//! Shared CONNECT policy and phase framing for blocking and Tokio transports.
//!
//! Session validation is strict; the underlying [`crate::v5`] codecs remain
//! permissive. GSS-API, BIND, UDP ASSOCIATE, and SOCKS4 are not implemented.
//! Username/password is plaintext: use a protected transport when required.

use crate::error::Error;
use crate::v5::{
    AuthMethod, Command, Endpoint, MethodRequest, MethodSelection, Reply, ReplyCode, Request,
    USERNAME_PASSWORD_VERSION, UsernamePasswordRequest, UsernamePasswordResponse, VERSION,
};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

/// Exactly one method is offered, preventing an implicit downgrade to no-auth.
/// Credentials are borrowed and deliberately have no `Debug` implementation.
#[derive(Clone, Copy)]
pub enum ClientAuth<'a> {
    /// Explicitly permit an unauthenticated proxy connection.
    NoAuthentication,
    /// Require RFC 1929 username/password authentication.
    UsernamePassword {
        /// Username bytes (1–255 bytes).
        username: &'a [u8],
        /// Password bytes (1–255 bytes).
        password: &'a [u8],
    },
}

impl ClientAuth<'_> {
    pub(crate) fn method(self) -> AuthMethod {
        match self {
            Self::NoAuthentication => AuthMethod::NoAuthentication,
            Self::UsernamePassword { .. } => AuthMethod::UsernamePassword,
        }
    }

    pub(crate) fn credentials(self) -> Result<Option<UsernamePasswordRequest>, Error> {
        match self {
            Self::NoAuthentication => Ok(None),
            Self::UsernamePassword { username, password } => Ok(Some(
                UsernamePasswordRequest::builder()
                    .username(username.to_vec())
                    .password(password.to_vec())
                    .build()?,
            )),
        }
    }
}

type CredentialCheck = dyn Fn(&[u8], &[u8]) -> bool + Send + Sync;

/// Explicit server authentication policy. No authentication is never a fallback.
/// Callbacks must be fast, nonblocking, and must not panic or retain credentials.
/// Password storage, constant-time verification, and rate limiting belong to the caller.
#[derive(Clone)]
pub struct ServerAuth {
    check: Option<Arc<CredentialCheck>>,
}

impl ServerAuth {
    /// Explicitly allow unauthenticated sessions.
    #[must_use]
    pub const fn no_authentication() -> Self {
        Self { check: None }
    }

    /// Require username/password and accept only credentials approved by `check`.
    #[must_use]
    pub fn username_password(check: impl Fn(&[u8], &[u8]) -> bool + Send + Sync + 'static) -> Self {
        Self {
            check: Some(Arc::new(check)),
        }
    }

    pub(crate) fn method(&self) -> AuthMethod {
        if self.check.is_some() {
            AuthMethod::UsernamePassword
        } else {
            AuthMethod::NoAuthentication
        }
    }

    pub(crate) fn select(&self, offer: &MethodRequest) -> Result<AuthMethod, Error> {
        version(offer.version, VERSION)?;
        if offer.is_valid() && offer.methods.contains(&self.method()) {
            Ok(self.method())
        } else {
            Err(Error::NoAcceptableMethod)
        }
    }

    pub(crate) fn authenticate(&self, request: &UsernamePasswordRequest) -> bool {
        request.version == USERNAME_PASSWORD_VERSION
            && request.is_valid()
            && self
                .check
                .as_ref()
                .is_some_and(|check| check(&request.username, &request.password))
    }
}

/// Bounds used by the listening proxies and their TCP per-connection handlers.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Absolute deadline for negotiation, authentication, and receiving CONNECT.
    pub handshake: Duration,
    /// Budget shared by outbound connection attempts (not blocking system DNS).
    pub connect: Duration,
    /// Total relay lifetime in both transports, not an idle timeout.
    pub relay: Duration,
    /// Maximum simultaneous sessions accepted by a listening proxy.
    pub connections: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            handshake: Duration::from_secs(10),
            connect: Duration::from_secs(10),
            relay: Duration::from_secs(300),
            connections: 64,
        }
    }
}

impl Limits {
    pub(crate) fn validate(self) -> Result<(), Error> {
        if self.handshake.is_zero()
            || self.connect.is_zero()
            || self.relay.is_zero()
            || self.connections == 0
            || std::time::Instant::now()
                .checked_add(self.handshake)
                .is_none()
            || std::time::Instant::now()
                .checked_add(self.connect)
                .is_none()
            || std::time::Instant::now().checked_add(self.relay).is_none()
        {
            Err(Error::InvalidLimits)
        } else {
            Ok(())
        }
    }
}

type DestinationCheck = dyn Fn(SocketAddr, &Endpoint, SocketAddr) -> bool + Send + Sync;

/// Shared listener policy. Construction requires an explicit destination decision.
#[derive(Clone)]
pub struct ServerConfig {
    /// Authentication policy, without implicit downgrade.
    pub auth: ServerAuth,
    /// Resource bounds, validated before accepting work.
    pub limits: Limits,
    authorize: Arc<DestinationCheck>,
}

impl ServerConfig {
    /// Set authentication and a policy checked before dialing each resolved address.
    /// Arguments are the client's peer address, original destination, and exact socket
    /// address to dial. Inspect the resolved IP, not just the domain, to prevent DNS
    /// from bypassing an IP allowlist. Callbacks must be fast, nonblocking, and
    /// must not panic. A callback panic terminates a listener with `WorkerPanicked`.
    #[must_use]
    pub fn new(
        auth: ServerAuth,
        authorize: impl Fn(SocketAddr, &Endpoint, SocketAddr) -> bool + Send + Sync + 'static,
    ) -> Self {
        Self {
            auth,
            limits: Limits::default(),
            authorize: Arc::new(authorize),
        }
    }

    pub(crate) fn permits(
        &self,
        peer: SocketAddr,
        destination: &Endpoint,
        target: SocketAddr,
    ) -> bool {
        (self.authorize)(peer, destination, target)
    }
}

// Framing only: payload interpretation and encoding remain exclusively bnb's job.
// Each requested read ends at a known phase boundary; no tunnel bytes are consumed.
#[derive(Clone, Copy)]
pub(crate) enum Frame {
    Offer,
    Selection,
    Credentials,
    AuthReply,
    Command,
}

impl Frame {
    pub(crate) fn needed(self, bytes: &[u8]) -> Result<usize, Error> {
        let n = bytes.len();
        match self {
            Self::Selection | Self::AuthReply => Ok(2),
            Self::Offer | Self::Credentials if n < 2 => Ok(2),
            Self::Offer => Ok(2 + usize::from(bytes[1])),
            Self::Credentials => {
                let prefix = 3 + usize::from(bytes[1]);
                if n < prefix {
                    Ok(prefix)
                } else {
                    Ok(prefix + usize::from(bytes[prefix - 1]))
                }
            }
            Self::Command if n < 4 => Ok(4),
            Self::Command => match bytes[3] {
                1 => Ok(10),
                4 => Ok(22),
                3 if n < 5 => Ok(5),
                3 => Ok(7 + usize::from(bytes[4])),
                code => Err(Error::UnsupportedAddressType(code)),
            },
        }
    }
}

pub(crate) fn version(actual: u8, expected: u8) -> Result<(), Error> {
    if actual == expected {
        Ok(())
    } else {
        Err(Error::Version { expected, actual })
    }
}

pub(crate) fn reserved(value: u8) -> Result<(), Error> {
    if value == 0 {
        Ok(())
    } else {
        Err(Error::Reserved(value))
    }
}

pub(crate) fn endpoint(value: &Endpoint) -> Result<(), Error> {
    if matches!(value, Endpoint::Domain { name, .. } if name.is_empty() || name.len() > 255) {
        Err(Error::InvalidEndpoint)
    } else {
        Ok(())
    }
}

pub(crate) fn domain_name(bytes: &[u8]) -> Result<&str, Error> {
    let name = std::str::from_utf8(bytes).map_err(|_| Error::InvalidEndpoint)?;
    if name.is_empty() || name.contains('\0') {
        Err(Error::InvalidEndpoint)
    } else {
        Ok(name)
    }
}

pub(crate) fn selection(value: MethodSelection, offered: AuthMethod) -> Result<(), Error> {
    version(value.version, VERSION)?;
    match value.method {
        AuthMethod::NoAcceptable => Err(Error::NoAcceptableMethod),
        method if method == offered => Ok(()),
        method => Err(Error::UnexpectedMethod(method)),
    }
}

pub(crate) fn authentication(value: &UsernamePasswordResponse) -> Result<(), Error> {
    version(value.version, USERNAME_PASSWORD_VERSION)?;
    if value.status.is_success() {
        Ok(())
    } else {
        Err(Error::AuthenticationRejected)
    }
}

pub(crate) fn request(value: &Request) -> Result<(), Error> {
    version(value.version, VERSION)?;
    reserved(value.reserved)?;
    if value.command != Command::Connect {
        return Err(Error::UnsupportedCommand(value.command));
    }
    endpoint(&value.destination)
}

pub(crate) fn reply(value: &Reply) -> Result<(), Error> {
    version(value.version, VERSION)?;
    reserved(value.reserved)?;
    if value.code != ReplyCode::Succeeded {
        return Err(Error::Reply(value.code));
    }
    endpoint(&value.bound)
}

pub(crate) fn response(code: ReplyCode, bound: Endpoint) -> Reply {
    Reply {
        version: VERSION,
        reserved: 0,
        code,
        bound,
    }
}

pub(crate) fn failure(code: ReplyCode) -> Reply {
    response(code, SocketAddr::from(([0, 0, 0, 0], 0)).into())
}

pub(crate) fn timed_out() -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::TimedOut, "SOCKS deadline expired")
}
