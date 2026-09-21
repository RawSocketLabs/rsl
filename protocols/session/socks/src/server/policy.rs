//! One authentication and authorization policy shared by all accepted protocols.
//!
//! Callbacks must be fast, nonblocking, and non-panicking. Password storage,
//! constant-time verification, rate limiting, and transport protection belong to
//! the caller. Credential bytes are neither logged nor retained in request context.

use crate::{Destination, Version, error::Error};
use std::{net::SocketAddr, sync::Arc};

type CredentialCheck = dyn Fn(&[u8], &[u8]) -> bool + Send + Sync;

/// Explicit authentication requirement, without an unauthenticated fallback.
#[derive(Clone)]
pub struct ServerAuth {
    check: Option<Arc<CredentialCheck>>,
}

impl ServerAuth {
    /// Explicitly accept unauthenticated connections.
    #[must_use]
    pub const fn no_authentication() -> Self {
        Self { check: None }
    }

    /// Require credentials verified by this callback on every accepted protocol.
    #[must_use]
    pub fn username_password(check: impl Fn(&[u8], &[u8]) -> bool + Send + Sync + 'static) -> Self {
        Self {
            check: Some(Arc::new(check)),
        }
    }

    pub(crate) fn requires_credentials(&self) -> bool {
        self.check.is_some()
    }

    pub(crate) fn verify(&self, username: &[u8], password: &[u8]) -> bool {
        self.check
            .as_ref()
            .is_some_and(|check| check(username, password))
    }

    fn accepts(&self, outcome: Authentication) -> bool {
        matches!(
            (self.requires_credentials(), outcome),
            (false, Authentication::Unauthenticated) | (true, Authentication::UsernamePassword)
        )
    }
}

/// Authentication actually completed by a version-specific exchange.
/// This records assurance, not a principal. No username or password is retained;
/// an unverified claimed identity must never become `UsernamePassword`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Authentication {
    /// The exchange deliberately used no authentication.
    Unauthenticated,
    /// The shared credential verifier accepted a username/password exchange.
    UsernamePassword,
}

/// Application operation, independent of version-specific command codes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Operation {
    /// Establish a TCP connection to a destination.
    Connect,
}

/// Non-secret facts established by a successfully validated exchange.
/// Unsupported or malformed wire commands never reach authorization.
#[non_exhaustive]
pub struct RequestInfo {
    /// The protocol actually used, not just a configured preference.
    pub version: Version,
    /// Authentication completed before the request was accepted.
    pub authentication: Authentication,
    /// Requested application operation.
    pub operation: Operation,
    /// Original destination, before DNS, retaining opaque domain bytes.
    pub destination: Destination,
}

/// One authorization decision, evaluated before dialing an exact numeric target.
/// Inspect `target`, not only the original domain, to prevent DNS allowlist bypasses.
#[derive(Clone, Copy)]
#[non_exhaustive]
pub struct Context<'a> {
    /// The connected client's peer address.
    pub peer: SocketAddr,
    /// Protocol-neutral request and authentication facts.
    pub request: &'a RequestInfo,
    /// Exact resolved address that would be dialed if approved.
    pub target: SocketAddr,
}

type DestinationCheck = dyn Fn(Context<'_>) -> bool + Send + Sync;

/// Shared authentication requirements and destination authorization.
/// There is no permissive default. Clones share the same callback objects.
#[derive(Clone)]
pub struct Policy {
    auth: ServerAuth,
    authorize: Arc<DestinationCheck>,
}

impl Policy {
    /// Require explicit authentication and a decision for each resolved candidate.
    #[must_use]
    pub fn new(
        auth: ServerAuth,
        authorize: impl Fn(Context<'_>) -> bool + Send + Sync + 'static,
    ) -> Self {
        Self {
            auth,
            authorize: Arc::new(authorize),
        }
    }

    /// Authentication requirements used by every accepted version's exchange.
    #[must_use]
    pub fn authentication(&self) -> &ServerAuth {
        &self.auth
    }

    pub(crate) fn authorize_addresses(
        &self,
        peer: SocketAddr,
        request: &RequestInfo,
        mut addresses: Vec<SocketAddr>,
    ) -> Result<Vec<SocketAddr>, Error> {
        if !self.auth.accepts(request.authentication) {
            return Err(Error::PermissionDenied);
        }
        if addresses.is_empty() {
            return Err(Error::InvalidEndpoint);
        }
        addresses.retain(|target| {
            (self.authorize)(Context {
                peer,
                request,
                target: *target,
            })
        });
        if addresses.is_empty() {
            Err(Error::PermissionDenied)
        } else {
            Ok(addresses)
        }
    }
}
